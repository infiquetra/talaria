# Repair brief — issue #111: documentation and version (cycle 3)

Reviewed revision: `3016f177a8b07949eb1e59a9b64f000b01a892b3` (the Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Previous cycles: `122bd918…` (brief set at `docs/code-reviews/2026-08-31-v0-5-0-repairs/`) and `83ffd27a…` (`docs/code-reviews/2026-08-31-v0-5-0-cycle2-repairs/`).
Review outcome: **`cycle_cap_best_available`**. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/d6f674a2cf79cd35a9f74bb3ed9884ca8d1fc88b9bbd80251b79c4180cebcb5b.md`.

**This round is not scored.** The engine reached its three-cycle cap and will not run a fourth scored cycle. The operator has decided to repair all sixteen fix requests before release — the full set, P2 and P3 alike, not a subset. Nothing here is a threshold chase: every request is a defect somebody measured.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review, or either earlier brief set. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(carried — not in the sixteen)* are **not part of the approved round**. The consensus engine excludes pre-existing and advisory findings from consolidation, so they produced no fix request. They are recorded so they are not lost, and repairing one is a scope decision for the coordinator, not for you.

## The one thing every lane must check before starting

**Does your request touch `talaria/`?** Each request below says so explicitly, and it is the most consequential fact in this brief.

Acceptance is complete and binding at this revision: candidate `788fc791` and revision `3016f177` have byte-identical `talaria`, `pyproject.toml`, `uv.lock` and `src` trees, verified by tree hash, so the wheel that was tested is the wheel under review. A repair confined to documentation, tests and the harness **preserves** that. A repair touching `talaria/` changes the shipped wheel and **invalidates all 43 receipts**, forcing both testers to re-drive the complete checklist.

Six of the sixteen touch product code. Ten do not. The coordinator is sequencing one re-drive, not two — so if your request touches `talaria/`, your landing time matters to somebody else's week.

## Verification standard for this round

This review has now found **five repairs across three cycles that satisfied their written acceptance criterion while leaving the defect alive**, and two of the sixteen requests below are defects a previous repair introduced. The architecture lens established the mechanism: guards here are habitually written against a literal the author typed rather than against the shape the system actually produces, and derivation stops one directory short of the population that matters.

So: **prove your repair by falsification, and prove it against real inputs.** Break the thing you fixed and watch the named test go red. Where a corpus exists — 44 live receipts, 133 quarantined receipts, an eight-row band table, a four-member verdict constant — point your test at the corpus, not at a string typed beside the assertion. A test whose fixture you wrote in the same sitting as the expectation cannot fail for the reason you wrote it.

## Requests (3 approved)

| Request | Severity | Findings | Route | Touches `talaria/`? | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-566116938dc4` | P2, P3 | F-103, F-105 | `manual` -> `review-fixer` | no — **preserves the receipts** | — |
| `fix-1104964b879f` | P3 | F-108 | `safe_auto` -> `review-fixer` | no — **preserves the receipts** | — |
| `fix-58827a218378` | P3 | F-83 | `manual` -> **`human`** | no — **preserves the receipts** | — |

**No request in this brief touches `talaria/`.** This lane is entirely off the re-drive path and can land at any time.

**One request is owned by `human`, not `review-fixer`.** `fix-58827a218378` is the engineering-journal gap. It is a writing task requiring judgement about what this release actually learned, and the review is explicit that it should not be produced mechanically.

**A note on what this lane already achieved, because it is the largest single move in the review.** Documentation and clarity scored 6.42, then 6.25, then **7.83** — a 1.58-point rise in one cycle, the biggest of any lens across all three, after being the only lens to fall in cycle 2. All three of its sub-floor dimensions cleared: `completeness-audience-prerequisites` 5.0 to 7.5, `runbook-safety-rollback-links-generated-drift` 5.5 to 7.5, and `terminology-cross-document-consistency` 3.5 to 7.0. The two documents the operator scoped back in did the work they were scoped in to do.

---

## `fix-566116938dc4` — P2, P3 (F-103, F-105)

Route `manual` -> owner `review-fixer`. Preserves the receipts.

**Exact paths.** `docs/install.md`

### F-103 — P2 — the recovery procedure cannot fix the failure it documents

**What is wrong.**

The new install guide's troubleshooting section documents a real failure — typing `talaria` reaches a stale copy that exits without output — and offers exactly one remedy: reorder `PATH` so the directory reported by `uv tool dir --bin` comes first.

**On the machine this release was developed and accepted on, that remedy cannot work, because the stale copy *is* the thing in that directory.** Measured directly:

- `command -v talaria` returns `/Users/jefcox/.local/bin/talaria`.
- `uv tool dir --bin` returns `/Users/jefcox/.local/bin` — the same directory, so the guide's comparison at `:94-95` shows no discrepancy in exactly the failing case.
- That file is a symlink into a uv tool environment holding a build so old it does not recognise `--version`: running `"$(uv tool dir --bin)/talaria" --version` prints `talaria: error: unrecognized arguments: --version`.

So the precondition the remedy at `:100` is gated on — "If the last two commands print `talaria 0.5.0`" — is false precisely when the reader needs it. They run four commands, all of which agree, and reach the end of the section with no next step. The commands that actually resolve a stale uv tool install — `uv tool uninstall`, `uv tool upgrade`, `uv tool install --force` — appear nowhere in the document; a grep for `uninstall`, `--force` and `upgrade` returns nothing.

The guide also mis-names the cause, describing the stale copy as something that shadows "the executable installed by uv" when it **is** the executable installed by uv.

**What to change.**

Split the failure into its two real shapes and give each a working remedy:

- **A stale executable in some other `PATH` directory.** Keep the existing comparison, which correctly identifies this case when the two values disagree, and keep the `PATH` remedy.
- **A stale uv tool environment**, which is the case the guide's own anecdote describes. The two values agree, and the remedy is `uv tool install --force git+https://github.com/infiquetra/talaria@v0.5.0`, or `uv tool uninstall talaria` followed by a fresh install.

Tell the reader which case they are in, based on whether the two directory values agree, **before** offering either remedy.

### F-105 — P3 — the guide never warns about the first-run credential prompt

**What is wrong.**

The guide's stated contribution includes its failure modes, and its troubleshooting section documents two. Neither is the one a first-time reader is most likely to hit: install Talaria, type `talaria` before running `talaria refresh-credential`, and the process stops at `Hermes gateway session token: ` with no default shown, no hint about the pairing command, and no visible way out.

Measured in a pseudo-terminal made the process's controlling terminal, with an empty configuration directory: the prompt appeared, a Ctrl+C byte was written, and **thirteen seconds later the process had not exited**. `talaria/transport/credentials.py:320` runs `getpass.getpass` on a worker thread via `asyncio.to_thread`, which is why the interrupt does not reach it.

The underlying behaviour is pre-existing and established, and this finding does not contradict that — it is about the **new document**, which names failure modes as part of its purpose and omits this one. The happy-path ordering is correct, so a careful reader is safe; a reader who follows "Verify the installation" and then simply runs the program has no way to recognise what happened.

Worth including for the reader: with **no** controlling terminal, Talaria exits cleanly with an error naming the fix, so the hang is specific to an interactive launch.

**Verifiably resolved when (both).**

- The stale-command section names both shapes, keys the choice on whether `command -v talaria` and `uv tool dir --bin` agree, and gives a remedy for each that works in that case.
- Every command the section tells a reader to run is one that exists — check `uv tool --help` rather than trusting the prose.
- A third troubleshooting entry, or a sentence closing "Prepare the gateway credential", covers the hidden prompt: it does not clear on Ctrl+C, leave it with Ctrl+D or by closing the terminal, run `talaria refresh-credential` first.
- The full project check is green.

---

## `fix-1104964b879f` — P3 (F-108)

Route `safe_auto` -> owner `review-fixer`. Preserves the receipts.

**Exact paths.** `docs/00-index.md`, `tests/docs/test_v050_release_docs.py`

**What is wrong.**

The index gained a Releases section this cycle and it lists a single entry. `docs/releases/` holds five files, v0.1.0 through v0.5.0.

This bites the exact reader the new release notes are written for: `docs/releases/v0.5.0.md:3` says the notes are "for people moving from v0.4.0", and `README.md:60` sends readers to the v0.4.0 release note for the standing Linux and credential limits — but a reader starting at the documentation index has no route to it. The test that pins the section, `test_documentation_index_reaches_the_install_and_release_guides`, asserts only that the literal strings `install.md` and `releases/v0.5.0.md` appear among the link targets, so nothing will notice the omission.

A smaller instance of the same gap: `docs/formats/status-line.md` declares `Authority: contract` and describes the operator status-command contract that `docs/configuration.md`'s `status.command` setting depends on, and it is unlinked — while both its sibling format documents are linked. That one is pre-existing; the release-notes gap is new with the section.

For the record, the link surface itself is sound: a link walker over eleven reader-facing documents found zero broken relative links. This is a coverage gap, not a broken-link defect.

**What to change.**

List all five release notes under the Releases heading, newest first. Add `docs/formats/status-line.md` to the user guides or a formats group in the same pass.

**Verifiably resolved when.**

- The pinning test **globs `docs/releases/*.md` and asserts every file is linked** — the same shape the evidence-index test already uses. A sixth release must not be able to land unreachable, and asserting one more literal string repeats the defect one release later.
- Every document under `docs/formats/` declaring `Authority: contract` is reachable from the index, enforced the same way.
- The full project check is green.

---

## `fix-58827a218378` — P3 (F-83)

Route `manual` -> owner **`human`**. Preserves the receipts.

**Exact paths.** `docs/engineering-journal/LEARNINGS.md`, `docs/engineering-journal/DECISIONS.md`

**What is right first, because it is most of the story.** The decisions half of this release's journalling is genuinely strong. `DECISIONS.md` gained 110 lines across two dated entries covering all eleven key technical decisions, the concurrency and candidate rules, explicit corrections to the planning record, a superseded implementation note, and a revisit-when condition. `QUEUED.md` gained 136. The gap is specific, not general neglect.

**What is wrong.**

`LEARNINGS.md`, `ARCHIVE.md`, `narratives/` and `audits/` are **byte-identical** between the diff base and this revision — `git diff` over all four returns empty output across the whole release scope. The newest learning entry is dated 2026-08-18, twelve days before the v0.5.0 run plan.

`docs/engineering-journal/README.md` assigns "empirical findings, mechanisms, fixes, validations, and generalizable rules" to `LEARNINGS.md`, to be updated on "significant runs, bugs, audits, or surprising test results", **in the same change**. This release produced exactly that material and recorded none of it:

- A priority-one write escape through a symlink in the theme saver, and its `follow_symlinks` default-deny resolution.
- A keyboard collision in which Talaria's F6 and F7 aliases were consumed by the composer's text area — now in its third cycle.
- A documented behaviour change to the status bar's narrow band.
- A guard that matches zero of the 133 real inputs it was written for, and the one-directory-short glob that explains it.

Across the entire cycle-2 to cycle-3 range the journal received **one line**: a line-number correction inside `QUEUED.md`.

Separately, two decisions taken during the repair cycles are recorded nowhere durable. Talaria now **overrides** `TEXTUAL_ANIMATIONS` rather than honouring it — `talaria/ui/app.py:1209` changed from `none if reduced_motion else self.animation_level` to `none if reduced_motion else "full"` — which is a deliberate choice between two named alternatives with an accessibility consequence. Grepping `talaria/`, `docs/` and `tests/` for `TEXTUAL_ANIMATIONS` returns hits only in `tests/ui/test_motion.py` and in the dated review briefs, never in the journal or the configuration guide. The narrow-band status bar change is documented in the user guide, which is right, but not recorded as a decision.

The cost is the one the journal exists to prevent: the next maintainer who sees an operator's `TEXTUAL_ANIMATIONS` setting ignored has no record that it was deliberate.

**What to change.**

One dated `LEARNINGS.md` entry per non-obvious fix, in the repository's existing **Evidence / Mechanism / Generalizable rule** shape, covering at minimum the four items above. Then two short `DECISIONS.md` entries: that Talaria sets its animation level from `ui.reduced_motion` alone and overrides any inherited value, naming the rejected alternative; and that the 20-to-31 column band now starts `task_progress` compact and lets overflow demote it, replacing the pinned-minimum rule.

**Verifiably resolved when.**

- `LEARNINGS.md` carries a 2026-09 entry for each of the four, each with Evidence, Mechanism and a one-line Generalizable rule.
- The generalizable rule for the quarantine guard is the one the architecture lens named and is worth writing down for its own sake: **a guard's input must come from the corpus or the constant the system produces, never from a string typed beside the assertion.**
- `DECISIONS.md` records both repair-cycle decisions with their rejected alternatives.
- No test is required and none should be added; this is a writing task.
