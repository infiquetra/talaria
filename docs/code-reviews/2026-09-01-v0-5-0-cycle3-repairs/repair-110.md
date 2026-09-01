# Repair brief — issue #110: acceptance harness (cycle 3)

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

## Requests (4 approved, 2 carried)

| Request | Severity | Findings | Route | Touches `talaria/`? | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-6303809f1f55` | P2, P3 | F-80, F-94, F-95, F-96, F-107 | `manual` -> `review-fixer` | no — **but forces a re-drive anyway; see below** | — |
| `fix-2cf72800a064` | P2, P3 | F-89, F-99 | `safe_auto` -> `review-fixer` | no — preserves the receipts | — |
| `fix-c241b40f2213` | P3 | F-82 | `gated_auto` -> `review-fixer` | no — preserves the receipts | — |
| `fix-7571eedc5ed2` | P2 | F-104 | `manual` -> `review-fixer` | no — preserves the receipts | — |
| `F-93 (carried — not in the sixteen)` | P3 | F-93 | `safe_auto` -> `review-fixer` | no | — |
| `F-97 (carried — not in the sixteen)` | P3 | F-97 | `advisory` -> `downstream-resolver` | no | — |

**No request in this brief touches `talaria/`. One of them still forces a re-drive, and it is the most important thing in this document.**

`fix-6303809f1f55`'s F-94 half asks for a **required** `harness_commit` field in `docs/acceptance/v0.5.0/receipt.schema.json`, written by the driver at drive time. Every one of the 44 existing receipts lacks that field and would fail validation against the new schema. So this request invalidates the evidence on its own, independently of the product-code split, and must be sequenced **with the six product-code repairs**, not with the documentation ones. The coordinator is cutting one candidate for all of it.

**Hard ordering inside this brief.** Take `fix-6303809f1f55` before `fix-c241b40f2213`. Both rewrite `v050_common.py`, `v050_records.py` and `v050_receipt.py`; the quarantine and environment repairs restructure the modules the verdict extraction then edits.

**Hard ordering reaching outside.** `fix-6303809f1f55`'s F-107 half regenerates the acceptance summary sentence that `README.md` and `CHANGELOG.md` quote verbatim. Issue #105's `fix-ff4bb42eaf9e` edits the changelog. Yours goes first.

---

## `fix-6303809f1f55` — P2, P3 (F-80, F-94, F-95, F-96, F-107)

Route `manual` -> owner `review-fixer`. **Forces a re-drive via the receipt schema.**

**Exact paths.** `scripts/acceptance/v050_receipt.py`, `scripts/acceptance/v050_common.py`, `scripts/acceptance/v050_records.py`, `scripts/acceptance/v050_pty_driver.py`, `scripts/acceptance/test_v050_harness.py`, `tests/docs/test_v050_acceptance_records.py`, `docs/acceptance/v0.5.0/receipt.schema.json`, `docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md`, `README.md`, `CHANGELOG.md`

### F-94 — the quarantine guard matches none of the receipts it exists to catch

**This is the fifth repair across three cycles to satisfy its written criterion while leaving the defect alive, and it is the one that revealed why.**

Cycle-2 finding F-71 asked for a machine-checkable marker so quarantined acceptance evidence could not be silently restored and adopted. The repair took the fallback route: a helper at `scripts/acceptance/v050_receipt.py:489` scanning four path fields inside each receipt for a directory component named `superseded`.

Quarantining is done by **moving directories**, not by rewriting receipt contents. So every quarantined receipt still names the active repository path it was written with. Measured: the helper returns false for **all 133** quarantined receipts across all ten bundles. Two lenses ran that census independently and agree.

The consequence was then measured end to end, running the exact acceptance criterion cycle 2 wrote: copy the two quarantined bundles for the previous candidate over the active evidence for both testers, regenerate, verify. It printed **`valid`, exit 0** — precisely the outcome the criterion says must no longer happen.

The regression test that pins this, `test_verify_run_rejects_superseded_origin_in_active_receipt`, hand-edits a synthetic receipt to inject the string `superseded` into a path field — a shape occurring in **zero** of the 133 real receipts. Green test, live defect.

**The architecture lens found the mechanism, and it is worth understanding before you fix it.** Every enumerator in this repository, production and test alike, uses the same one-level glob `*/receipts/*.json` — the record generator at `v050_records.py:228`, the validator at `v050_receipt.py:714`, the schema test at `tests/docs/test_v050_acceptance_records.py:138`. That pattern matches 42 active files and zero quarantined ones, because the 133 sit two levels deeper at `evidence/<tester>/superseded/<commit>/receipts/`. **Derivation stops exactly one directory short of the only population this guard exists to police.**

There is a real compensating control, and it works: the release workflow passes the released commit, and on the restored tree `verify-run --expect-candidate 788fc791` errored with `manifest candidate does not describe the released commit`. So a restored bundle cannot reach a tagged release. What it can pass is every check a person runs by hand, and the in-suite test that runs the verifier against the real committed evidence.

**Take the primary route this time, not the fallback.** Add a required `harness_commit` field to `docs/acceptance/v0.5.0/receipt.schema.json`, written by the driver from the repository head at drive time, and have the verifier reject any receipt whose harness commit is not the current head or an ancestor with no difference over `scripts/acceptance`. This is what forces the re-drive, and it is worth it: the harness at this revision already differs from the harness that produced the evidence by 187 lines across four files, with no receipt recording which wrote it.

### F-95 — the child environment is still built by subtraction

Cycle 1 found the driver exporting terminal width and height. Cycle 2 found every `TEXTUAL_` variable inherited. **Both are genuinely fixed** — the testing lens ran a real Textual child on a real pseudo-terminal with thirty-six hostile parent variables and got framework defaults identical to a clean-parent run. Do not disturb that.

But the shape that produced both is unchanged: `isolated_environment` at `scripts/acceptance/v050_common.py:129` still starts from a copy of the whole parent environment and subtracts. Eighteen hostile variables still reach the child, and two change its behaviour, isolated one at a time: `UNICODE_VERSION=8.0.0` makes the party-popper and watch emoji measure one column instead of two, and `PYTHONOPTIMIZE=2` strips assert statements from the child.

**Exposure today is nil, and the lens said so plainly:** no file in the shipped package and no active capture contains any of the 10,239 characters whose width differs between the tables, and the shipped package contains zero assert statements. This is a durability gap, not a defect in this release's evidence — but it is the third cycle in which the same subtraction pattern leaked a rendering control, and each time the leaking name was one nobody had thought of.

**Invert to an allow-list**, the way `talaria/status/contract.py` already builds its child environment: start empty and copy forward only what the child needs.

### F-96 — a missing-file refusal no test pins

`scripts/acceptance/v050_receipt.py:345` raises a named refusal when a screenshot does not exist. Replacing that condition with one that is never true leaves the harness test file at exactly its baseline. Two sibling mutations in the same surface **are** killed — deleting the screenshot from the digest-verified tuple gives 3 failures, neutralising the scratch-directory confinement gives 2 — so the repair for cycle-2 F-72 mostly took. This branch is what survives. A tester pointing at a missing screenshot would still fail, via `FileNotFoundError` a few lines later, but with a traceback instead of the harness's own named refusal. There is no `capture.is_file()` counterpart at all.

### F-107 — the summary's four numbers do not reconcile

A reader **can** tell that covered slots and receipt files are different quantities — the sentence names them differently and "separately" does real work, so the cycle-2 conflation is genuinely fixed. What a reader cannot do is reconcile them: 43 expected slots, all 43 covered, 44 current receipts, 42 item and 2 install. Adding 42 and 2 gives 44, one more than the 43 slots those receipts cover, and nothing explains the difference.

The cause is a real overlap: checklist item 1 is shared, so it has two slots, and `talaria-t2` satisfies its slot twice — once through an install receipt, once through an item receipt — while `talaria-t1` satisfies its slot only through an install receipt. The evidence table hides this too: row 1 renders `PASS | PASS` with no indication that the left cell comes from `_install_status_cell` and the right from `_status_cell`. This sentence is quoted verbatim into `README.md` and `CHANGELOG.md`, so the unexplained arithmetic is on the release's front page.

Add one **generated** clause naming the overlap, or have the generator emit the reconciliation explicitly.

**Verifiably resolved when (all five).**

- **Copy a real quarantined bundle from `docs/acceptance/v0.5.0/evidence/*/superseded/` into a temporary active tree, run `refresh` then `verify-run`, and confirm it is refused with a message naming the superseded origin.** Use a real bundle, not a fabricated path shape. This exact sequence currently returns `valid`, and it is the criterion cycle 2 wrote and this repair missed.
- `harness_commit` is `required` in the receipt schema and a receipt lacking it is rejected by a test.
- The child-environment test asserts an **unknown** parent variable does not reach the child, so the next unanticipated name is caught by the suite rather than by a fourth review.
- Deleting the screenshot existence check turns a named test red. Report which.
- The summary's four numbers reconcile on the page, and the clause is generated rather than typed.
- The full project check is green.

---

## `fix-2cf72800a064` — P2, P3 (F-89, F-99)

Route `safe_auto` -> owner `review-fixer`. Preserves the receipts.

**Exact paths.** `scripts/acceptance/v050_receipt.py`, `scripts/acceptance/test_v050_harness.py`, `docs/acceptance/v0.5.0/artifact-manifest.schema.json`

### F-89 — 44 published files never reach the privacy scanner

The scanner walks only the evidence directory. `docs/acceptance/` holds 1012 files; the gate's own scan expression yields 968. The 44 that are never examined are **six recorded terminal frame logs** under `corpora/t1/`, thirty-three event scripts and two READMEs under `event-scripts/`, the artifact manifest, two schemas, the checklist and one results document.

The frame logs are the worst class to miss: they are verbatim recordings of Hermes traffic, exactly where an unredacted field would land. The lens confirmed the gap is the scan set and not the detector — appending an operator home path and an email address to a frame log produced **zero** errors from the gate, while handing that same file directly to the detector produced both. `scripts/acceptance/test_v050_harness.py:1262` pins the scan to the evidence directory, so the gap is asserted rather than merely inherited.

For the record, the cycle-2 repair otherwise worked: the byte scan of all 1012 files came back clean on every pattern, against 336 leaking files in cycle 2, and all 287 PNGs carry only `IHDR`, `IDAT` and `IEND`.

**Widening is safe today** — the detector over all 44 currently-unscanned files reports zero errors — so this cannot break the build.

### F-99 — the manifest schema gives its counts no meaning

Every one of the nine `counts` properties is a bare `{"type": "integer", "minimum": 0}` with no description, and `expected_receipts: 43` sits adjacent to `current_receipts: 44` under names that invite direct comparison. They are incommensurable — one counts (checklist item, tester) slots, the other counts receipt files including the two install receipts. A release-gating tool binding to this schema alone reads 44 against 43 and reaches the wrong conclusion. The generated prose gets this right, which shows the project understands the distinction and did not carry it into the machine-readable contract.

**Verifiably resolved when.**

- The privacy sweep walks the acceptance **release root**, and a test plants a leak in a `corpora/` frame log and asserts it is caught. Keep the evidence-root parameter for the manifest binding checks, which legitimately need it.
- Every `counts` property carries a description, and a test asserts that — so a later added count cannot land undescribed.
- The schema states that coverage is `expected_receipts - missing_current_receipts`.
- The full project check is green.

---

## `fix-c241b40f2213` — P3 (F-82)

Route `gated_auto` -> owner `review-fixer`. Preserves the receipts. **Depends on `fix-6303809f1f55`.**

**Exact paths.** `scripts/acceptance/v050_common.py`, `scripts/acceptance/v050_records.py`, `scripts/acceptance/v050_receipt.py`, `scripts/acceptance/test_v050_harness.py`

**What is wrong.**

Cycle-2 finding F-60 asked for one authority for the four acceptance verdict words. The repair delivered one authority **for membership testing**: `VERDICTS` at `scripts/acceptance/v050_common.py:16`, imported by both consumers. Enumeration and schema were left behind, so the vocabulary now has **five** copies rather than four:

- `scripts/acceptance/v050_records.py:414` iterates a hardcoded four-tuple to build the manifest's `item_verdicts` block — in the same file that imports `VERDICTS` twenty lines earlier.
- `scripts/acceptance/v050_receipt.py:160` restates them as prose: `verdict must be pass, fail, blocked, or reserved`.
- `docs/acceptance/v0.5.0/receipt.schema.json:30` restates them as a JSON Schema enum.
- `docs/acceptance/v0.5.0/artifact-manifest.schema.json:128-133` restates them as required property names under `additionalProperties: false`.

Measured: adding a fifth member to `VERDICTS` on a disposable copy left the harness and documentation suites at **exactly** the baseline, byte-identical including which test failed. No test in the tree references `VERDICTS` at all. The trap has a specific shape — a new verdict would be accepted by the validator, rejected by the receipt schema, silently omitted from the manifest counts, absent from the operator-facing error message, and would produce a manifest failing its own schema.

**What to change.**

Give the vocabulary an **order** as well as a membership — an `ORDERED_VERDICTS` tuple with `VERDICTS` derived from it — and drive `v050_records.py:414` and the error message at `v050_receipt.py:160` from it.

**Verifiably resolved when.**

- A test loads both JSON schemas and asserts the verdict enumeration and the `item_verdicts` property names **equal the constant**. Derive from the constant; do not restate it.
- Widening `VERDICTS` by one member now fails loudly. Run that probe and report which test caught it — it currently changes nothing.
- The full project check is green.

---

## `fix-7571eedc5ed2` — P2 (F-104)

Route `manual` -> owner `review-fixer`. Preserves the receipts.

**Exact paths.** `docs/acceptance/v0.5.0/evidence/t1/README.md`, `tests/docs/test_v050_release_docs.py`

**What is wrong.**

This is the mechanism of cycle-2 finding F-46 surviving in a second document: prose written at one moment asserting what the generated manifest says, then never updated when the other tester's evidence landed.

`docs/acceptance/v0.5.0/evidence/t1/README.md:53-54` reads "The generated manifest still flags the T2 receipts currently on this branch as stale; combined cross-tester completion becomes true only after the parallel T2 half is merged."

At this revision the manifest reports **zero** stale receipts and zero invalid ones. All twenty T2 item receipts carry `stale_candidate` false and every one binds to the current candidate `788fc791`. The T2 `item-36` receipt — the "parallel T2 half" the sentence says is not yet merged — is committed in this same tree, and was recorded at 03:01:47Z against the T1 half's 03:24:52Z, so it already existed when the sentence was written.

A reader reaching this file from the documentation index, which links it as the T1 evidence index, concludes the cross-tester check is unproven and half the evidence is stale — when the release's own manifest says the opposite. Nothing catches it: the only test touching these files asserts they are *linked*, not that their prose agrees with the manifest.

**What to change.**

Rewrite the two clauses to the state the manifest reports. Then close the gap that let it survive: extend `tests/docs/test_v050_release_docs.py` so the tester evidence indexes are checked for **agreement with the manifest**, at minimum that no evidence index asserts a stale or missing-receipt condition the manifest's counts contradict.

**Verifiably resolved when.**

- No evidence index asserts a condition the manifest's counts contradict, and a test enforces that by **reading the manifest**, not by matching a typed string.
- Corrupting a manifest count makes that test fail. Run the probe.
- The full project check is green.

---

## Carried, not in the sixteen

### `F-93` — P3 — dependency surveillance covers actions only

Route `safe_auto` -> `review-fixer`. Pre-existing.

The action-pinning work is complete and was taken beyond what cycle 2 asked: all twelve `uses:` lines carry a 40-character digest and `.github/dependabot.yml` was added. But it declares only the `github-actions` ecosystem. The repository also commits `uv.lock` — 33 packages, 206 artifact digests — and `package-lock.json`, both installed authoritatively in CI, and neither has any advisory surveillance. A search for `pip-audit`, `npm audit`, `safety`, `osv`, `trivy`, `grype`, `snyk`, `sbom`, `cyclonedx`, `attest`, `provenance`, `sigstore` and `cosign` across the workflows and manifests returns nothing.

The lockfiles guarantee CI installs exactly the reviewed artifacts, which is the more important property and is satisfied. What is missing is any signal when one of those pinned artifacts is later found vulnerable. **Add a `uv` and an `npm` ecosystem entry** on the same monthly schedule, if the coordinator scopes it in.

### `F-97` — P3 — an upstream collection defect the error message misattributes

Route `advisory` -> `downstream-resolver`. Pre-existing. **Advisory: no code change unless the coordinator decides.**

Running `tests/ui/test_status_bar.py`, `tests/ui/test_theme.py`, `tests/test_config.py` and `tests/ui/test_transcript_bounds.py` together in that order gives 118 passed and 15 errors, each reading `fixture 'stress_frames' not found`, with none of `tests/ui/conftest.py`'s fixtures visible. Every pairing and every module alone passes.

**It is not Talaria's.** `--trace-config` shows the conftest **is** registered as a plugin in the failing run, so it is not shadowing, not an import failure, not a rootdir effect. A five-file, ten-line synthetic project with no Talaria code reproduces it exactly on the same pytest 9.1.1. The trigger needs a `tests/ui` file, then a file directly in `tests/`, then another `tests/ui` file.

**The full run passes by design, not by luck:** `testpaths = ["tests", "scripts/acceptance"]` gives two directory arguments, and a directory-tree walk visits each directory once, so the duplicate-node condition cannot arise. Three independent full runs agree at 2517 passed.

The defect the review charges is **diagnostics**: the error names Talaria's own fixture while the cause is upstream, and nothing redirects the reader — even though the repository already does exactly that for Textual at `tests/ui/test_blocks.py:340`. `pyproject.toml:50` declares an open-ended `pytest>=8.3` while the lock resolves 9.1.1, and no test records the audited runner version.

**Do not move `tests/ui/conftest.py`.** The layout is correct, and reshaping it around a third-party defect trades good structure for bad.
