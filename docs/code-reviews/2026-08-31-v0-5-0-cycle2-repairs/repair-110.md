# Repair brief — issue #110: acceptance harness (cycle 2)

Reviewed revision: `83ffd27addc6df4cbdb73bc996baa7d11a2610f3` (the unmerged Talaria v0.5.0 integration candidate, after the cycle-1 repair round).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Previous cycle: `122bd918e0056404e576ae5623ce9e97bfe1ad93`, brief set at `docs/code-reviews/2026-08-31-v0-5-0-repairs/`.
Review outcome: `repairs_requested`, cycle 2 of a maximum 3. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/657c776e2ec70ded65e838487eabf9439f4326bfc7e857a8ab999328fedcd7b3.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review, or the cycle-1 brief set. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost.

**Priority tiers.** Every request carries a tier, and the tier is the coordinator's ordering, not a severity restatement:

- **Tier 1 — sub-floor dimension.** The review's acceptance rule has a hard floor: every applicable dimension must score at least 7.0. Five dimensions are below it. A tier-1 request is the named lever on one of them, so it is a concrete quality gap rather than a threshold miss. Do these first.
- **Tier 2 — remaining P1.** A priority-one finding whose dimension is already above the floor.
- **Tier 3 — everything else.**

The five sub-floor dimensions, with the lens that scored them: `specification-documentation-parity` 6.5 (interface contract), `keyboard-focus` 6.0 (accessibility), and three under documentation and clarity — `completeness-audience-prerequisites` 5.0, `terminology-cross-document-consistency` 3.5, and `runbook-safety-rollback-links-generated-drift` 5.5.

## Requests (10)

| Request | Tier | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-0d833481e8ee` | **1** | P1, P2 | F-47, F-63 | `manual` -> `review-fixer` | — |
| `fix-7dba9d4183ec` | **1** | P1 | F-48 | `manual` -> `release` | — |
| `fix-e88e3801ea3e` | 2 | P1, P3 | F-45, F-71 | `gated_auto` -> `review-fixer` | — |
| `fix-beaf4a6e3917` | 3 | P2, P3 | F-51, F-66 | `gated_auto` -> `release` | — |
| `fix-db4b03d8b310` | 3 | P2 | F-53 | `safe_auto` -> `review-fixer` | — |
| `fix-04efdfe28cd1` | 3 | P2 | F-54 | `manual` -> `review-fixer` | — |
| `fix-6032fde41c6f` | 3 | P2, P2, P3 | F-57, F-60, F-72 | `safe_auto` -> `review-fixer` | — |
| `F-67 (no fix request — pre-existing)` | 3 | P3 | F-67 | `manual` -> `review-fixer` | — |
| `F-68 (no fix request — pre-existing)` | 3 | P3 | F-68 | `safe_auto` -> `human` | — |
| `F-75 (advisory — no fix request)` | 3 | P3 | F-75 | `advisory` -> `downstream-resolver` | — |

This is the largest brief in the set. Cycle 1's release blocker was here and it is closed: the acceptance-evidence gate passes at this revision, receipts are current and complete, and the package under test is byte-identical to the package under review. What remains is that the harness's own record still contradicts itself in prose, and that two of its guards do not guard what they claim to.

**Ordering — hard, and it reaches outside this brief.**

```
fix-0d833481e8ee  -->  fix-7dba9d4183ec  -->  #111 fix-d1bddcdb324b (README and changelog)
```

Take `fix-0d833481e8ee` first: it deletes `_observations`, the function that reads the acceptance results document back in as input. `fix-7dba9d4183ec` restructures that same document by moving prose sections inside generated markers. Restructuring the document while a function still parses it is how a generator eats its own output.

Then both must land before issue #111 rewrites the README and the changelog, because that rewrite quotes the acceptance verdict. If it goes first, it propagates a second stale claim and cycle 3 raises the same finding with different words. This is the same dependency cycle 1 recorded, in the same direction, for the same reason.

**Soft dependency from issue #109.** F-78 there decides whether Talaria honours `TEXTUAL_ANIMATIONS` or overrides it. Ask before writing `fix-e88e3801ea3e`'s regression test, so it asserts the default the application actually produces.

**What the cycle-1 repair traded away, so you do not oscillate.** Commit 31c46f6 widened the Bandit gate to cover `scripts/` — which cycle-1 finding 9 asked for — and added `-ll` in the same edit to silence the noise that widening produced. Coverage went up and strictness went down together, and the security lens's score fell 8.38 to 8.20 partly on that. **Do not repair F-53 by reverting to `bandit -r talaria -q`**: that throws away the coverage. Split the invocation so each tree keeps the strictness it needs.

---

## `fix-0d833481e8ee` — Tier 1 — P1, P2 (F-47, F-63) — take first

Route `manual` -> owner `review-fixer`

Sub-floor dimension: `runbook-safety-rollback-links-generated-drift` = **5.5** against a floor of 7.0. This request and nothing else moves it.

**Exact paths.**

- `scripts/acceptance/v050_records.py`
- `docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md`
- `tests/docs/test_v050_acceptance_records.py`
- `docs/acceptance/v0.5.0/receipt.schema.json`

### F-47 — P1 — the generated table carries observations that contradict their receipts

**What is wrong.**

`scripts/acceptance/v050_records.py:444-456` — `_observations(matrix)` parses the observation column out of the **existing document** and raises only when an item number is missing, calling the result a "hand-written observation". The generator recomputes each verdict cell from the receipts and copies the observation cell forward verbatim from the previous rendering of the same table, with no check that the two agree.

Stale prose is therefore laundered through a block labelled generated. Fifteen of thirty-six rows now assert something their own receipt denies:

- Row 84 — item 23 is `PASS` with the note "a genuine reconnect cycle never displays the required `[~]` form". Its receipt records "the raw capture contains both connection lost — reconnecting and `[~]` retry before recovery".
- Row 75 — item 14 is `PASS` with the note "T2 shows resize-driven status repainting is broken". `item-14-talaria-t2.json` records "the real child reflowed at every breakpoint from 144 through 19 columns".
- Thirteen further `PASS` rows carry "The earlier T1 receipt is superseded; no current evidence exists."

`tests/docs/test_v050_acceptance_records.py` passes 18 of 18 at this revision, because `check_records` compares verdict cells and manifest counts and never looks at the observation column.

A reader who trusts the note concludes a shipped feature is defective. A reader who trusts the verdict cannot tell which other notes are stale.

**What to change.**

Make `observations` a required field in `docs/acceptance/v0.5.0/receipt.schema.json` first. Then source the observation cell from each receipt's own `observations` array and delete `_observations` entirely. Extend `check_records` to report any row whose observation is not derived from the current receipt.

### F-63 — P2 — the release gate document declares no gate block

**What is wrong.**

`tests/docs/test_gating_documents.py` opens by naming the incident it was written for: a release-gate document whose two conditions were met by later work with nothing pointing back. The repository therefore already owns an anti-staleness convention built for exactly this failure — and the most consequential gating document in the release does not use it. A grep for the gate fence across `docs/` returns two documents, and the acceptance results is not one of them.

Because no gate block names item 23 and item 24 as blocking conditions, nothing failed when the item-23 receipt turned to `pass`, and the verdict outlived its own evidence table by seven hours and fifteen commits. The two checks that would have caught it go quiet for a document with no gate block.

**What to change.**

Add a fenced gate block at `docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md:8` with an identifier, a verdict, a review-by horizon, and one blocks-on line per unmet checklist item — currently item 24 alone.

**Verifiably resolved when (both findings).**

- `grep -n '_observations' scripts/acceptance/v050_records.py` returns nothing.
- Regenerate the document and confirm all fifteen contradicting rows now carry text present in their own receipt. Report the count that changed.
- Hand-edit one observation cell to text absent from its receipt, run `check_records`, and confirm it **fails**. This is the assertion that does not currently exist; a passing regeneration alone does not prove the check works.
- `observations` is `required` in `docs/acceptance/v0.5.0/receipt.schema.json`, and a receipt lacking it is rejected by a test.
- The document carries a gate block naming item 24, and `uv run pytest tests/docs/test_gating_documents.py -q` picks it up — verify by flipping item 24's row to a cleared grade and confirming the gate test fails.
- The full project check is green.

---

## `fix-7dba9d4183ec` — Tier 1 — P1 (F-48) — take second

Route `manual` -> owner `release`

Sub-floor dimension: `terminology-cross-document-consistency` = **3.5** against a floor of 7.0 — the lowest score anywhere in this review. This request and issue #111's `fix-d1bddcdb324b` are the two levers on it.

**Exact paths.**

- `docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md`
- `scripts/acceptance/v050_records.py`
- `tests/docs/test_v050_acceptance_records.py`

**What is wrong.**

One document states the release verdict three incompatible ways and names two different candidate commits, one of which now exists at this revision only under a superseded directory.

- Lines 10-14, hand-written: "NOT SATISFIED. T1 installed candidate commit 17ce4eda... item 23 failed... every T2 receipt still binds to candidate d9c82443".
- Line 28, generated: current reviewed candidate commit `0f5c8e3e`. Line 27: manifest status `BLOCKED`. Line 31: 43 current, 0 stale, 0 invalid.
- Line 84, generated: item 23's T1 cell is `PASS`.
- Lines 120-124, hand-written: "NOT SATISFIED ... item 23 still fails ... all T2 items also lack current-candidate evidence".

Both `17ce4eda` and `d9c82443` exist only under `docs/acceptance/v0.5.0/evidence/*/superseded/`. A reader cannot determine which statement is authoritative without regenerating the records themselves, and the document offers no instruction for doing that.

**The terminology ledger is shared.** Issue #111's brief carries a four-term ledger — the outcome word, "candidate commit", "receipt" and "evidence", and "superseded" — naming which meaning is authoritative for each. Read `docs/code-reviews/2026-08-31-v0-5-0-cycle2-repairs/repair-111.md` before rewriting any prose here, and resolve the four terms the same way. The two documents are scored on the same dimension and a divergent resolution fixes neither.

**What to change.**

Move the Status and Final verdict sections inside generated markers driven by the manifest's own status and counts, so the generator rewrites them alongside the provenance and verdict blocks. Extend `check_records` to fail when either differs from the manifest.

**Verifiably resolved when.**

- Every commit identifier appearing anywhere in the document names a commit that exists outside a `superseded/` directory, or is explicitly labelled as superseded in the same sentence.
- Hand-edit the Status line to disagree with the manifest, run `check_records`, and confirm it **fails**. Then restore. Without that probe the new markers are decoration.
- The document states the verdict exactly once, and the words come from the manifest's own status field rather than being composed by hand.
- The document names the command that regenerates it.
- The full project check is green.

---

## `fix-e88e3801ea3e` — Tier 2 — P1, P3 (F-45, F-71)

Route `gated_auto` -> owner `review-fixer`

**This is the request the coordinator most wanted answered this cycle.** The question put to the review was whether the acceptance harness can still hide a failure the way the `COLUMNS` and `LINES` export did. The answer is yes, through a different variable, and it lands on the exact setting cycle 1's reduced-motion finding was about.

**Exact paths.**

- `scripts/acceptance/v050_common.py`
- `scripts/acceptance/test_v050_harness.py`
- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/v050_pty_driver.py`
- `docs/acceptance/v0.5.0/receipt.schema.json`

### F-45 — P1 — the acceptance child inherits every `TEXTUAL_` variable

**What is wrong.**

`scripts/acceptance/v050_common.py:106` builds the child environment as `dict(os.environ)`, then removes every name with a `TALARIA_` prefix by loop, then pops a fixed ten-name list: `PYTHONHOME`, `PYTHONPATH`, `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`, `COLORTERM`, `COLUMNS`, `LINES`, and one Hermes token. There is no `TEXTUAL_` prefix loop.

Textual reads `TEXTUAL_ANIMATIONS`, `TEXTUAL_THEME`, `TEXTUAL_SMOOTH_SCROLL`, `TEXTUAL_FPS`, `TEXTUAL_DRIVER`, `ESCDELAY`, `TERM_PROGRAM` and `LC_TERMINAL`. None is stripped. The testing lens ran the harness's own `run_pty` with a real Textual child: a parent carrying `TEXTUAL_ANIMATIONS=none TEXTUAL_THEME=nord TEXTUAL_SMOOTH_SCROLL=0 TEXTUAL_FPS=3` produced child output `level=none theme=nord smooth=False fps=3`, against `level=full theme=textual-dark smooth=True fps=60` from a clean parent.

`TEXTUAL_ANIMATIONS` is the same defect class as `COLUMNS` and `LINES` through a different channel, and it is not an abstract one. `talaria/ui/app.py:1209` sets `self.animation_level = "none" if reduced_motion else self.animation_level`, leaving the inherited value on the non-reduced path. So a reduced-motion acceptance leg run on a machine with that variable set cannot distinguish a working `ui.reduced_motion` from the cycle-1 defect.

Two things make this squarely repairable. The repository already solves it correctly elsewhere: `build_child_env` in `talaria/status/contract.py` is default-deny, with `tests/status/test_env.py` asserting `TEXTUAL_DRIVER` is stripped. And the author knew the technique — the `TALARIA_` prefix loop sits three lines above.

**What to change.**

Prefer inverting to default-deny over a named base set, the way `build_child_env` already does. If that is too large for this round, add a `TEXTUAL_` prefix loop beside the existing `TALARIA_` one and pop `ESCDELAY`, `ROWS`, `TERM_PROGRAM`, `LC_TERMINAL` and `TERMINFO` explicitly; set `TERM_PROGRAM` from the value already passed as `--terminal-program`, so the child's terminal identity matches the receipt.

### F-71 — P3 — superseded evidence carries no machine-checkable marker

**What is wrong.**

Quarantine is a directory convention and nothing more. The receipts under `superseded/driver-pinned-dimensions/` carry the same candidate commit, wheel digest, redaction attestation and verdict as the active ones. Nothing in a receipt says it was produced by a driver that pinned the terminal size.

The testing lens proved the consequence: copying one superseded receipt alone produced three errors because its evidence paths dangle, but restoring the full bundle and re-running `refresh` then `verify-run` produced `valid`. The only distinguishing signal is free text in the terminal-program field.

**What to change.**

Add a required harness-commit field to `docs/acceptance/v0.5.0/receipt.schema.json`, written by the driver from the repository head at drive time, and reject a receipt whose harness commit predates the driver fix. Failing that, add a check that no active receipt path contains a `superseded` component.

**Verifiably resolved when (both findings).**

- Re-run the lens's probe: export `TEXTUAL_ANIMATIONS=none` and `TEXTUAL_THEME=nord` in the parent, drive a real child through the harness, and assert the child reports the framework defaults (`level=full`, `theme=textual-dark`). Add it as a test in the shape of the existing real-Textual resize test. A unit test over the environment dictionary is not sufficient — the resize defect had one of those.
- A test asserts the child's `TERM_PROGRAM` equals the value passed as `--terminal-program`, so the receipt's terminal identity is derived rather than inherited.
- Copy a full superseded bundle back into the active tree, run `refresh` then `verify-run`, and confirm it now **fails** with a message naming the superseded origin. That exact sequence currently returns `valid`.
- The full project check is green.

---

## `fix-beaf4a6e3917` — Tier 3 — P2, P3 (F-51, F-66)

Route `gated_auto` -> owner `release`

**Exact paths.**

- `.github/workflows/release.yml`
- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/test_v050_harness.py`
- `docs/acceptance/v0.5.0/evidence/t1/install-receipt.json`
- `docs/acceptance/v0.5.0/artifact-manifest.json`

### F-51 — P2 — the release gate never binds evidence to the released commit

**What is wrong.**

`.github/workflows/release.yml:144-147` runs `verify-run` with no expected-commit argument. `scripts/acceptance/v050_receipt.py:537` takes `expected_commit` from the manifest's own candidate field — which is whatever the operator typed into `refresh`. The check therefore proves the receipts agree with the manifest, not that either describes the artifact being released. A grep for `GITHUB_SHA`, `rev-parse` or `merge-base` across the workflows returns nothing for the acceptance step. `scripts/acceptance/v050_records.py:667` has `_git_is_ancestor`, but it is reached only during a drive, never from `verify_run`.

At this revision the manifest candidate is `0f5c8e3e` and the reviewed commit is `83ffd27a`. The review controller established by tree hash that `talaria`, `pyproject.toml`, `uv.lock` and `src` are identical between them, and that is the only reason the evidence is valid here. **The gate did not establish it.** A release of one commit can ship with internally consistent acceptance evidence for an unrelated commit and the workflow prints `valid`.

### F-66 — P3 — 336 evidence files carry the operator's temporary directory identifier

**What is wrong.**

A byte scan of all 791 files under the acceptance tree found 1,047 matches across 336 files, all one distinct Darwin per-user temporary root — a path segment derived from the account identifier and stable for that account on that machine. The highest concentration is the eight install receipts at fourteen occurrences each.

This is a weak but durable per-operator identifier published in a repository whose own instructions forbid private operational context, and it correlates this evidence with any other artifact carrying the same segment. It is materially less sensitive than the cycle-1 leak and exposes no credential — which is why it is P3.

For the record, the cycle-1 leak is gone: zero occurrences of `/Users/`, `/home/`, the operator username, any email address, hostname, bearer token or key-shaped string, and the install receipts now read a placeholder. PNG chunk enumeration across 226 images found only `IHDR`, `IDAT` and `IEND`.

**What to change.**

For F-51: give `verify-run` an `--expect-candidate` argument that errors unless the manifest candidate equals it, or is an ancestor of it with an empty diff over `talaria/`, and pass the release SHA from the workflow.

For F-66: extend the publishing serialiser to substitute a scratch-root placeholder, resolving the tester scratch root from the install receipt and replacing it the way the repository root and home already are. Regenerate the affected receipts and the manifest digests.

**Verifiably resolved when.**

- A test passes `verify-run` a divergent expected commit and asserts the error. Then a second test passes an ancestor commit with an empty `talaria/` diff and asserts it is accepted, so the ancestor allowance is pinned rather than assumed.
- `.github/workflows/release.yml` passes the release SHA to that argument.
- The byte scan over `docs/acceptance/` returns zero matches for the temporary root. State the count before and after.
- The manifest digests are regenerated and `verify-run` is `valid` on the regenerated tree.
- The full project check is green.

---

## `fix-db4b03d8b310` — Tier 3 — P2 (F-53)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `.github/workflows/validate.yml`
- `.github/workflows/release.yml`
- `pyproject.toml`

**What is wrong.**

Both workflows changed from `bandit -r talaria -q` to `bandit -r talaria scripts -q -ll`. The `-ll` flag raises the reporting floor to medium severity, and Bandit's hardcoded-credential checks all declare LOW. The security lens planted a `GATEWAY_PASSWORD` constant in `talaria/`: the new invocation exits 0; the cycle-1 command exits 1 on B105.

All 72 findings under the loosened invocation are low and live in `scripts/`, so the flag hides nothing in the code as it stands today. The defect is the gate, not present code — a hardcoded password, token or key committed into `talaria/` no longer fails the check that would previously have caught it. The flag appears at `validate.yml:78`, `validate.yml:178` and `release.yml:142`.

**What to change.**

Split the invocation so each tree keeps the strictness it needs: `bandit -r talaria -q` unchanged, plus `bandit -r scripts -q -ll`. Or keep one command and silence the low noise at its source with a per-directory skip in `pyproject.toml`, rather than raising the global floor. Do not drop the `scripts/` coverage — cycle-1 finding 9 asked for it.

**Verifiably resolved when.**

- Re-run the lens's probe: plant a hardcoded credential constant in `talaria/`, run the workflow's exact Bandit command, and confirm it exits non-zero. Then remove it. Report the check identifier that fired.
- The same probe planted in `scripts/` behaves as the coordinator intends, and the intent is stated in a comment beside the command.
- All three call sites carry the same policy.
- The full project check is green.

---

## `fix-04efdfe28cd1` — Tier 3 — P2 (F-54)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/test_v050_harness.py`

**What is wrong.**

The only mechanical confidentiality control over committed evidence is machine-relative. `scripts/acceptance/v050_receipt.py:57` computes `home_path = home or str(Path.home().resolve())` and is called with no argument at `:116` and `:599`. The security lens set `HOME` to `/home/runner` and `_contains_home_path` returned `False` for the exact cycle-1 leak string; with the operator's home it returns `True`.

`.github/workflows/release.yml:143-146` runs `verify-run` on a hosted runner, where that home can never match a path authored on the operator's account. The exact leak cycle 1 found would pass silently there.

It is also narrow. `scripts/acceptance/v050_receipt.py:521-522` globs `*/receipts/*.json` and `*/install-receipt.json`, which a single star cannot extend into the superseded tree, so every superseded receipt is unscanned. A grep for `ansi`, `png`, `corpora` and `jsonl` across that module returns no scanning code, so the ANSI captures, the screenshots and the corpora are unscanned too.

**What to change.**

Replace the machine-relative comparison with a portable pattern scan over the raw bytes of every file the manifest names. Add ANSI and JSONL to the sweep and PNG ancillary-chunk enumeration for images. Change both globs to double-star so the superseded tree is covered. Keep the home-path check as an additional local signal rather than the only one.

**Verifiably resolved when.**

- A test sets `HOME` to a value that cannot match, plants the cycle-1 leak string in a receipt, and asserts the scan **fails**. That case currently passes.
- A test plants the same string in a superseded receipt, in an ANSI capture, and in a JSONL corpus file, and asserts each is caught.
- A test plants a text comment in a PNG ancillary chunk and asserts it is caught.
- The full project check is green.

---

## `fix-6032fde41c6f` — Tier 3 — P2, P2, P3 (F-57, F-60, F-72)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `scripts/acceptance/v050_records.py`
- `scripts/acceptance/v050_common.py`
- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/test_v050_harness.py`

### F-57 — P2 — `refresh` accepts a candidate commit absent from the repository

`scripts/acceptance/v050_records.py:722` adds the required candidate-commit argument with no type or existence check; `:54-59` validates only the forty-hex shape. A typo or a commit from another repository produces a manifest naming a commit that does not exist, and `check` afterwards returns clean because it re-derives its expectation from the manifest's own recorded commit. Measured on a disposable copy: `refresh` with forty zeros produced status `stale`, 43 stale receipts, 0 current — and `check` then returned clean.

It degrades loudly rather than silently, so it cannot manufacture a passing record. But the operator's first signal is 43 spurious stale receipts rather than a message naming the bad argument. `_git_is_ancestor` at `:697` proves the repository is already reachable from this module.

**Fix.** Run `git cat-file -e` on the commit in the repository root before calling `refresh`, and raise a named error when it fails.

### F-60 — P2 — the acceptance verdict vocabulary is duplicated across four sites

`scripts/acceptance/v050_records.py:292` raises on an unknown verdict; `scripts/acceptance/v050_receipt.py:142` appends to an error list for the same set. `v050_receipt.py:668` repeats the set as argparse choices, and `:648` and `:719` carry two further terminal-verdict literals. `scripts/acceptance/v050_common.py:12-25` already defines `RELEASE_VERSION`, `TESTERS` and `FALLBACK_REASON_CODES`, establishing the convention this bypassed. The `_string` and `_object` helpers are copy-pasted between the two modules as well.

Adding a verdict today requires finding four literals across two modules, and the same rule fails two different ways.

**Fix.** Add `VERDICTS` and `TERMINAL_VERDICTS` frozensets to `v050_common.py` and import them at all four sites, passing `sorted(VERDICTS)` to argparse. Move the shared helpers there too. Make the two modules report an unknown verdict the same way, or state in a comment why they differ.

### F-72 — P3 — screenshot verification branches are asserted only via the capture

`scripts/acceptance/v050_receipt.py:255-262` verifies capture and screenshot in one loop over a two-element tuple. But `scripts/acceptance/test_v050_harness.py:637` unlinks only the capture and `:640` asserts the capture-labelled message; `:657` and `:660` do the same for the hash case. No test in the file asserts on either screenshot message. A change that dropped the screenshot entry from that tuple, or compared the screenshot against the capture's hash, would leave both tests green. The screenshot is the half of the evidence a human actually looks at.

**Fix.** Parametrise both tests over capture and screenshot so each label's missing-file and hash-mismatch branch is asserted independently.

**Verifiably resolved when (all three).**

- `refresh` with a non-existent commit exits with a message naming the argument, and a test asserts that message. Confirm `refresh` writes no manifest in that case.
- `grep -c "'pass'" scripts/acceptance/*.py` shows the verdict vocabulary in one module.
- Delete the screenshot entry from the two-element tuple at `v050_receipt.py:255-262`, run the harness tests, and confirm they now **fail**. Then restore. That deletion currently leaves them green.
- The full project check is green.

---

## `F-67 (no fix request — pre-existing)` — Tier 3 — P3 (F-67)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/test_v050_harness.py`

**What is wrong.**

Cycle 1 named two harness guards that no automation exercised. The refusal to touch the operator's real configuration directory now runs in the default suite — that half is closed. The other has not moved: the check at `scripts/acceptance/v050_receipt.py:303-308` that a raw capture and a screenshot came from inside the tester's scratch directory, which is what stops an arbitrary file being copied into the public evidence tree, still has no test. A grep for both message strings across `scripts/` and `tests/` matches only the module itself. The harness test module imports three names from that module, so the record path containing these guards is never entered.

A future edit dropping either check leaves the suite green.

**What to change.**

Extract the two checks into a module-level helper and add two tests calling it with a capture and with a screenshot under a temporary path outside the scratch root, asserting the two error messages.

**Verifiably resolved when.**

- Delete each check in turn, run the harness tests, and confirm each deletion turns them red. Report which test caught which.
- Both message strings now appear in `scripts/acceptance/test_v050_harness.py`.
- The full project check is green.

---

## `F-68 (no fix request — pre-existing)` — Tier 3 — P3 (F-68)

Route `safe_auto` -> owner `human`

**Exact paths.**

- `.github/workflows/release.yml`
- `.github/workflows/validate.yml`

**What is wrong.**

Every third-party action is referenced by a mutable major-version tag rather than a commit digest — `release.yml:85`, `:111`, `:120`, and nine sites in `validate.yml` — while `release.yml:34-35` grants `contents: write` at workflow scope. If an action's tag is repointed by upstream compromise, the next release run executes attacker code with write access and nothing in the run would look different.

This is the one supply-chain input not hash-pinned. The lockfile was parsed: 33 packages, every one from the public index with a hash except the project itself, and CI installs with a locked sync. The diff base carries the identical action references, so this is pre-existing and not introduced by the v0.5.0 work — which is why the engine excluded it from consolidation and why the owner is `human` rather than `review-fixer`.

**Note for the coordinator.** This is the only lever on the `dependency-supply-chain` dimension, which scores 8.0. See the reachability note in the dispatch manifest: security is one of two lenses that can reach 9.0, and it cannot do so without this.

**What to change.**

Pin each action to a full commit digest with the version as a trailing comment, and enable dependency updates for the actions ecosystem so the digests are bumped by reviewed pull request.

**Verifiably resolved when.**

- `grep -nE 'uses: [^@]+@v[0-9]' .github/workflows/*.yml` returns nothing.
- Every `uses:` line carries a 40-character digest and a trailing version comment.
- A dependency-update configuration covers the `github-actions` ecosystem.

---

## `F-75 (advisory — no fix request)` — Tier 3 — P3 (F-75)

Route `advisory` -> owner `downstream-resolver`

**Exact paths.**

- `docs/acceptance/v0.5.0/artifact-manifest.schema.json`
- `scripts/acceptance/test_v050_harness.py`

**What is wrong.**

The version-two schema expresses exactly one invariant and leaves every count unconstrained against the array it summarises. Its only conditional branch keys off the not-run status and constrains array cardinality; `counts` appears nowhere in it. The interface-contract lens ran a real validator against seven deliberately inconsistent manifests and all seven were accepted, including every receipt marked stale beside a zero stale count, and a complete status beside five failures.

Cycle 1's named case is now rejected and the validation now runs in the suite with two dedicated rejection tests, so the repair did land. The class is not covered. In practice the byte-for-byte regeneration check in the generator is what actually enforces consistency, so the residual risk is narrow: the schema is the artifact a consumer outside this repository would reach for, and it does not carry the guarantee.

This is **advisory**: it produces no code change unless the coordinator decides the schema should carry the invariant.

**What to change.**

Add the invariants the schema can express. Or — and this is the honest minimum — state plainly in the schema's own description that cross-field consistency is enforced by the generator's check and not by this document, so a consumer knows validation alone is insufficient.

**Verifiably resolved when.**

- Either the seven inconsistent manifests the lens constructed are rejected by a validator run in the suite, or the schema description says in one sentence that it does not check counts against arrays and names what does.
