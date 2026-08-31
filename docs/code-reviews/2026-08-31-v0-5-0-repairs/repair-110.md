# Repair brief — issue #110: acceptance harness

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (7)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-e920ba479916` | P1, P2 | F-10, F-13, F-30 | `manual` -> `release` | — |
| `fix-8f43cd8eaaf9` | P1 | F-9 | `gated_auto` -> `review-fixer` | — |
| `fix-b9a75043ad5e` | P1, P2 | F-12, F-31 | `manual` -> `review-fixer` | — |
| `fix-a8b6b76bb8cf` | P1, P2 | F-7, F-32 | `manual` -> `release` | — |
| `fix-d4c6f02783f5` | P2 | F-16 | `gated_auto` -> `release` | — |
| `fix-7795189b39a5` | P2 | F-28 | `manual` -> `release` | — |
| `F-42 (advisory — no fix request)` | P3 | F-42 | `advisory` -> `release` | — |

**Ordering.**

- `fix-e920ba479916` **must land before** `fix-7795189b39a5`. The manifest-schema invariant is designed to reject the manifest as it stands, so the acceptance rerun must populate candidate and receipts first or the new schema fails on landing.

---

## `fix-e920ba479916` — P1, P2 (F-10, F-13, F-30)

Route `manual` -> owner `release`

**Exact paths.**

- `docs/acceptance/v0.5.0/artifact-manifest.json`
- `docs/acceptance/v0.5.0/evidence/t2/receipts/`
- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/test_v050_harness.py`
- `.github/workflows/release.yml`

**What is wrong.**

**This is the release blocker.** Three artifacts describe the same acceptance run and contradict each other, and no code reads two of them together.

**F-13 (P1).** All fifteen committed `talaria-t2` receipts carry `"commit": "d86979127f871a479eb104fc10c886b5c5480a8c"`. The reviewed revision is `122bd918e0056404e576ae5623ce9e97bfe1ad93`, fourteen commits later, and product code changed in between: commit 85a224c, "fix(inspector): render honest empty tasks state", removed ten lines from `talaria/domain/changes.py`. KTD10 in the run plan (line 332) says a repair invalidates earlier verdicts, rebuilds the wheel, and reruns affected plus smoke flows. Nothing was rerun. `item-17-talaria-t2.json` records `"verdict": "fail"` with an observation naming the exact defect that commit repaired, so the release's only verdict for the repaired surface still says fail.

**F-10 (P1).** `docs/acceptance/v0.5.0/artifact-manifest.json` reads `"status": "not-run"`, `"candidate": null`, `"receipts": []` beside those fifteen receipts and sixty-two evidence files. Its own schema makes `candidate` and `receipts` required for exactly this binding purpose. It was last written at commit dc78dda on 2026-08-30, before the evidence landed at 20b9894 on 2026-08-31. A grep for `artifact-manifest` across `talaria/`, `tests/` and `scripts/` returns nothing, so no code populates or checks it.

**F-30 (P2).** Every receipt's `capture_path` points into `/private/var/folders/.../T/talaria-v050-talaria-t2-tdxgj8kp/raw/` and no `.ansi` file is committed anywhere. Hash verification passes today only because that temporary directory still exists on one machine. The twenty-two screenshots **are** committed and hash-match their receipts, so the image half of the chain is reproducible and the byte-capture half is not.

**What to change.**

Rebuild the wheel from `122bd918e0056404e576ae5623ce9e97bfe1ad93`, rerun the `talaria-t2` track against it — item 17 first, plus the inspector smoke items 16 and 18 — and add new receipts bound to that commit and wheel digest. **Do not edit the existing receipts**: KTD10 makes a receipt immutable evidence about one candidate, so the `fail` at d869791 stays and a new pass at 122bd918 supersedes it.

Populate the manifest from the receipts: set `status`, fill `candidate` with the rerun commit, wheel filename, wheel digest and version, and add one entry per receipt with path, digest, checklist item, tester and verdict. Add a `verify-run` subcommand to `scripts/acceptance/v050_receipt.py` that errors when the manifest candidate is null while receipts exist, when a receipt's `artifact.commit` differs from the manifest candidate, or when a receipt is absent from the manifest, and run it in the release workflow before the tag step.

Copy the `.ansi` captures into `docs/acceptance/v0.5.0/evidence/t2/raw/` beside the screenshots, rewrite each `capture_path` as a repository-relative path, and resolve receipt paths relative to the repository root so verification works from any checkout.

**Verifiably resolved when.**

- Every committed receipt's `artifact.commit` equals the candidate commit named in the manifest.
- `verify-run` exits non-zero on a manifest whose candidate is null while receipts exist, and on a receipt whose commit differs — demonstrate both failures before fixing the data.
- `find docs/acceptance/v0.5.0 -name '*.ansi' | wc -l` is greater than zero, and `validate_receipt(verify_files=True)` passes from a fresh clone with no temporary directory present.
- Item 17 has a receipt bound to the reviewed candidate. If it passes, the old fail receipt remains in the tree, superseded rather than deleted.
- The release workflow runs `verify-run` before the tag step.

---

## `fix-8f43cd8eaaf9` — P1 (F-9)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `pyproject.toml`
- `.github/workflows/validate.yml`
- `.github/workflows/release.yml`
- `scripts/acceptance/test_v050_harness.py`

**What is wrong.**

`pyproject.toml:70` sets `testpaths = ["tests"]`, so `uv run pytest` collects 2,338 tests and zero of them come from `scripts/acceptance/test_v050_harness.py`. Both `.github/workflows/validate.yml:76` and `.github/workflows/release.yml:139` run bare `uv run pytest`, which honours that setting. The reported green suite of 2,331 passed plus 7 skipped equals 2,338, so the harness tests are definitively outside it.

That leaves 1,854 lines of harness — the real `pty.fork()` driver, the install provenance probe, the receipt validator, and the two confidentiality guards that refuse to run against the operator's real `~/.talaria` and keep unredacted captures under tester scratch — guarded only by a 451-line test file that no automation runs. `scripts/acceptance/README.md:179` documents it as a manual command. The same configuration also leaves `scripts/` outside Bandit, because `pyproject.toml:78` excludes tests and the workflows invoke `bandit -r talaria`.

The evidence-producing machinery is the one thing in this release with no automated guard.

One known obstacle: `test_current_source_checkout_cannot_pose_as_an_installed_candidate` derives `_REPO_ROOT` from `Path(__file__).resolve().parents[2]` and asserts a message that only holds when that path is the virtual environment's editable-install target. Fix that coupling before enabling collection, or the change turns CI red for an environmental reason.

**What to change.**

Add `"scripts/acceptance"` to `testpaths` at `pyproject.toml:70`, or add a discrete `uv run pytest scripts/acceptance -q` step to both workflows. Extend the Bandit invocation to `uv run bandit -r talaria scripts -q`. Repair the checkout-location coupling first; if any harness test proves genuinely environment-sensitive in CI, give that one test an explicit skip condition rather than leaving the whole file uncollected.

**Verifiably resolved when.**

- `uv run pytest --collect-only -q | grep -c test_v050_harness` returns a non-zero count.
- All sixteen harness tests pass in CI, from a clean checkout, not only locally.
- `uv run bandit -r talaria scripts -q` runs in both workflows and reports no medium or high findings.
- Deleting the body of `validate_config_dir`'s refusal guard turns the suite red — check this locally and revert, so the guard is demonstrably covered.

---

## `fix-b9a75043ad5e` — P1, P2 (F-12, F-31)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/test_v050_harness.py`

**What is wrong.**

**F-12 (P1).** `validate_receipt` declares `verify_files: bool = True` at `scripts/acceptance/v050_receipt.py:84`, and the command-line path at `:438` uses that default. All four receipt tests pass `verify_files=False` — `scripts/acceptance/test_v050_harness.py:411`, `:425`, `:438` and `:450` — so the two error branches at `v050_receipt.py:218-225`, rejecting a missing capture file and a mismatched hash, have never executed under test. Hash verification is the mechanism binding a receipt to the bytes it describes; if it silently stopped working, every receipt would validate clean and the evidence chain would be decorative.

**F-31 (P2).** `validate_receipt` enforces model routes, redaction state, tester ownership and file hashes, but treats `artifact.commit` as an arbitrary present string — it appears only at `v050_receipt.py:117` among required fields and at `:304` where it is copied from the candidate manifest. Running the validator with file checks enabled over the committed item 3 and item 17 receipts, both bound to a superseded candidate, returned clean. The validator cannot distinguish fresh evidence from evidence taken against a build that no longer exists.

**What to change.**

F-12: add two tests that write real capture and screenshot bytes into `tmp_path`, point a receipt's evidence paths and digest fields at them, and assert `validate_receipt(receipt, verify_files=True)` returns clean for the matching case, `capture file is missing` after deleting the file, and `capture hash does not match its file` after appending a byte.

F-31: give `validate_receipt` an optional `expected_commit` parameter, thread it from the manifest's candidate through the command-line entry point at `:438`, and append the error `artifact.commit does not match the release candidate` when they differ.

**Verifiably resolved when.**

- Both error branches at `v050_receipt.py:218-225` are executed by a test — confirm with coverage or by inverting each condition locally and observing a failure.
- A test asserts a receipt with a wrong commit produces exactly the mismatch error, and one with the right commit does not. The helper at `test_v050_harness.py:366` currently hardcodes a placeholder commit, so it needs a real-commit variant.
- The harness tests are collected by the default invocation — this depends on fix-8f43cd8eaaf9.
- The full project check is green.

---

## `fix-a8b6b76bb8cf` — P1, P2 (F-7, F-32)

Route `manual` -> owner `release`

**Exact paths.**

- `docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md`
- `docs/acceptance/v0.5.0/evidence/`

**What is wrong.**

**F-7 (P1).** The top-level acceptance record says at line 4 "`talaria-t2` evidence remains pending", marks the `talaria-t2` install receipt `PENDING` at line 30, and marks every `talaria-t2` verdict cell `PENDING`. The same commit carries fifteen validated `talaria-t2` receipts recording thirteen passes, one failure (item 17) and one blocked item (item 34), and `docs/acceptance/v0.5.0/evidence/t2/README.md:23-31` narrates all of them. The document was last written at commit aee465b, which is **newer** than the 20b9894 commit that added the evidence, so this is a live inconsistency rather than an ordering artifact. A release decision made from that record sees no `talaria-t2` failure because it sees no `talaria-t2` result at all.

**F-32 (P2).** `docs/acceptance/v0.5.0/evidence/` holds a single entry, `t2`. `scripts/acceptance/v050_receipt.py:101-102` rejects any tester that is not `talaria-t1` or `talaria-t2`, so a `t1` track is a first-class concept, and `docs/acceptance/v0.5.0/checklist-items.json` assigns sixteen of the thirty-six items to `talaria-t1`. Those sixteen have no behavioural evidence in the repository, and a reader cannot tell whether they were executed and lost or never executed.

**What to change.**

Fill the `talaria-t2` column and the install-receipt row from the committed receipts, sourcing each cell from its file, and restate line 4 as fifteen executed items with thirteen passes, one failure and one blocked item. Add a link from the record to `docs/acceptance/v0.5.0/evidence/t2/README.md` so the two are navigably tied.

For the `t1` track: either commit its receipts under `docs/acceptance/v0.5.0/evidence/t1/`, or state in the record that the track was not executed for this candidate and list exactly which checklist items that leaves unevidenced.

**Verifiably resolved when.**

- No `PENDING` cell remains for a `talaria-t2` row that has a committed receipt — check by cross-referencing every filename under `evidence/t2/receipts/` against the matrix.
- The record's final verdict is consistent with the receipts it now cites.
- Either `evidence/t1/` exists with receipts, or the record names the unevidenced items explicitly.
- The record links the evidence README.

---

## `fix-d4c6f02783f5` — P2 (F-16)

Route `gated_auto` -> owner `release`

**Exact paths.**

- `docs/acceptance/v0.5.0/evidence/t2/install-receipt.json`
- `scripts/acceptance/v050_receipt.py`
- `scripts/acceptance/v050_install_probe.py`

**What is wrong.**

This repository is public and its own `CLAUDE.md` instructs "Keep this public repository free of private operational context and secrets." `docs/acceptance/v0.5.0/evidence/t2/install-receipt.json:24` commits `"integration_tree": "/Users/jefcox/workspace/infiquetra/orch-candidate"` — the operator's home directory, username and private workspace layout.

A home-path grep across the entire committed evidence tree returns exactly this one line, so the leak is bounded and its sensitivity is low. The problem is the process gap that produced it: there is no redaction function anywhere in `scripts/acceptance/`, and the only redaction control is the `redaction_review` attestation at `scripts/acceptance/v050_receipt.py:207-228`, whose stated scope is captures and screenshots. The install receipt sits outside that gate, which is exactly where the leak landed. `docs/acceptance/v0.5.0/evidence/t2/README.md:81` claims a search found no operator home paths — true for its scope, false for the committed artifact set.

Stated deliberately as negative evidence: no bearer token, API key, password, gateway URL or private email appears anywhere under `docs/acceptance/`; the rendered screenshots show no home path, credential or hostname; the images carry no text metadata.

**What to change.**

Replace the `integration_tree` value with a scrubbed placeholder such as `"<integration-tree>"` — its only live use is a containment check inside `probe_installed_artifact`, which runs against the live path and never reads the committed copy. Then close the gap: extend the redaction gate so `redaction_review` covers every committed artifact rather than captures and screenshots alone, and add a check refusing to validate any receipt or install receipt containing `str(Path.home())`. If the acceptance run is re-executed for fix-e920ba479916, add the scrub to `v050_install_probe.py` before the receipt is written rather than editing the committed file.

**Verifiably resolved when.**

- `git grep -nE '/Users/[a-z]|/home/[a-z]' -- docs/acceptance/` returns nothing.
- A test asserts `validate_receipt` rejects a receipt containing the current user's home path.
- The evidence README's redaction claim matches the scope actually checked.
- The full project check is green.

---

## `fix-7795189b39a5` — P2 (F-28)

Route `manual` -> owner `release`

**Exact paths.**

- `docs/acceptance/v0.5.0/artifact-manifest.schema.json`
- `pyproject.toml`

**What is wrong.**

`docs/acceptance/v0.5.0/artifact-manifest.schema.json:7` requires `schema_version`, `status`, `candidate` and `receipts` but imposes no relation among them — no `if`/`then`, no `dependentRequired`, no `allOf`. A manifest saying `status: not-run`, `candidate: null`, `receipts: []` is therefore schema-valid no matter what evidence sits beside it. Validated with `jsonschema` 4.26.0: the manifest produces zero errors, and all fifteen receipts produce zero errors against their own schema, and both schemas pass `check_schema`.

So the schema cannot gate manifest truthfulness, which is the only job a manifest has. And nothing in `scripts/`, `tests/` or `pyproject.toml` imports `jsonschema` or references either schema file, so even the validity these instances do have is unenforced and can regress silently.

**What to change.**

Add an `if`/`then` clause requiring `candidate` to be non-null and `receipts` to have `minItems: 1` whenever `status` is anything other than `not-run`, and requiring `receipts` to be empty when it is. Add a test that loads both schemas, runs `check_schema`, and validates the manifest plus every receipt under `docs/acceptance/v0.5.0/evidence/`, with `jsonschema` added to the development dependency group.

**Verifiably resolved when.**

- The current manifest, unchanged, now **fails** validation once fix-e920ba479916 has populated the receipts — that failure is the point of the invariant.
- A test validates every committed instance against its schema and runs in the default suite.
- The full project check is green.

---

## `F-42 (advisory — no fix request)` — P3 (F-42)

Route `advisory` -> owner `release`

**Exact paths.**

- `docs/acceptance/v0.5.0/evidence/t2/README.md`

**What is wrong.**

`docs/acceptance/v0.5.0/evidence/t2/README.md:23` records the item 17 failure — "Tasks invents a needs-you unavailable row instead of showing [none available from this session]" — and commit 85a224c in this same revision fixes exactly that defect without a re-drive. A reader cannot tell whether the failure stands, was repaired, or was repaired incorrectly. An acceptance record whose failures may or may not still be true cannot be used to close a release.

This is **advisory** and becomes unnecessary if fix-e920ba479916 reruns item 17; take it only as an interim measure if the rerun is deferred.

**What to change.**

Add a dated note under the failed-and-blocked section saying the item 17 defect was repaired by commit 85a224c after the run, naming `tests/ui/test_inspector.py::test_a_notice_only_queue_renders_the_tasks_empty_state` as the test that now covers it, and stating that item 17 requires a re-drive before it can be recorded as passing. Do not edit the immutable receipt.

**Verifiably resolved when.**

- The README states the repair, the covering test, and that a re-drive is still required.
- The item 17 receipt is byte-unchanged.

