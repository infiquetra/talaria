# Repair brief — issue #105: Visual Studio Code theme import (cycle 2)

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

## Requests (2)

| Request | Tier | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-ed280fc4431e` | **1** | P2 | F-55 | `safe_auto` -> `review-fixer` | — |
| `fix-80c6f0f8d6e0` | 3 | P2 | F-56 | `gated_auto` -> `review-fixer` | — |

**Why the first one matters more than its P2 label suggests.** `fix-ed280fc4431e` is the **only** request in the entire cycle-2 set that sits on `specification-documentation-parity`, the interface-contract dimension scoring 6.5 against a 7.0 floor. No other child can move that dimension. If this request is skipped, the interface-contract lens fails its floor in cycle 3 regardless of what else lands.

---

## `fix-ed280fc4431e` — Tier 1 — P2 (F-55)

Route `safe_auto` -> owner `review-fixer`

Sub-floor dimension: `specification-documentation-parity` = **6.5** against a floor of 7.0. This request is the sole lever on it.

**Exact paths.**

- `docs/formats/stored-theme.schema.json`
- `tests/ui/test_theme_import.py`
- `docs/formats/vscode-theme-import.md`

**What is wrong.**

`docs/formats/stored-theme.schema.json:7` lists `schema_version` in `required`. The runtime disagrees: `talaria/themes/storage.py:13-14` keeps it out of `_REQUIRED_FIELDS`, and line 58 defaults its absence to the current version. The companion page at `docs/formats/vscode-theme-import.md:166-168` explicitly promises that a stored theme without `schema_version` is treated as version one for one release.

So the schema — called normative by that same page — rejects exactly the document the page promises to keep loading. Measured with jsonschema 4.26.0: the importer's output is valid; the same document with `schema_version` removed is invalid, and loads at runtime regardless. A consumer validating an operator's existing theme files gets a false failure on a file Talaria opens without complaint, and the schema cannot answer the one migration question it was written for.

The suite cannot see this: `tests/ui/test_theme_import.py` re-implements the schema as hand-written assertions and never constructs a validator, so the schema and the code can disagree indefinitely with everything green.

**What to change.**

Move `schema_version` out of `required` and keep the `const` constraint in `properties`, so a present-but-wrong value is still rejected while an absent one is accepted. Add a comment naming the release in which it becomes required. Replace the hand-written assertions with a real `jsonschema` validator run against two documents: the importer's own output, and a legacy document with no `schema_version`.

**Verifiably resolved when.**

- A test constructs a real validator from `docs/formats/stored-theme.schema.json` and asserts a legacy document with no `schema_version` **validates**, and that a document with `schema_version` set to a wrong value **fails**. Both directions, or the constraint has simply been deleted.
- A third assertion loads that same legacy document through `talaria/themes/storage.py` and confirms the runtime accepts it, so the schema and the loader are pinned to each other rather than each to a hand-written expectation.
- `grep -c 'assert ' tests/ui/test_theme_import.py` no longer accounts for the schema's field list.
- The full project check is green.

---

## `fix-80c6f0f8d6e0` — Tier 3 — P2 (F-56)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/theme_import.py`
- `talaria/cli.py`
- `docs/formats/vscode-theme-import.md`
- `tests/test_cli.py`

**What is wrong.**

Two gaps in the machine-readable import route, both measured.

`talaria/ui/theme_import.py:253-274` — `to_json_dict` never reads `self.composites`, while the prose path's `records()` emits one composite line per `AlphaComposite`. Alpha compositing is the one lossy transformation the importer performs. Measured with an alpha source colour: the prose stdout carried the composite line and the JSON object had no `composites` key at all. A script consuming the JSON route cannot learn that a colour was flattened.

`talaria/cli.py:254-260` — the error handler prints prose and returns before the `json` branch, so a failed import with `--json` writes zero bytes to stdout. Measured: a malformed import with `--json` gave exit 3 and an empty stdout. A consumer that deliberately chose the machine-readable route must still parse English on stderr for every failure, distinguishing seven documented causes through a three-way exit bucket.

`docs/formats/vscode-theme-import.md` documents the object's fields and mentions neither omission.

**What to change.**

Add a `composites` array mirroring the existing `fallbacks` array. Emit a versioned error object on stdout when `--json` is set, carrying the `exc.kind` value that already exists, so all seven causes are machine-distinguishable rather than three. Document both in `docs/formats/vscode-theme-import.md`.

**Verifiably resolved when.**

- A test imports a theme with an alpha source colour under `--json` and asserts the parsed object's `composites` array has the same length as the prose path's composite line count for the same input — not merely that the key exists.
- A test runs a malformed import under `--json`, parses stdout as JSON, and asserts the object carries the specific `kind` for that failure. Then a second malformed input with a different cause asserts a **different** `kind`, so the seven causes are actually distinguished rather than collapsed into one error shape.
- `docs/formats/vscode-theme-import.md` documents both the `composites` array and the error object, including the `kind` vocabulary.
- The full project check is green.
