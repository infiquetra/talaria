# Repair brief — issue #107: right inspector

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (1)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-8fb716f69948` | P1 | F-4 | `gated_auto` -> `review-fixer` | — |

---

## `fix-8fb716f69948` — P1 (F-4)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/inspector.py`
- `tests/ui/test_inspector.py`

**What is wrong.**

`Inspector.toggle` writes `requested_collapsed` from inside its narrow-overlay branch — `talaria/ui/inspector.py:329` sets it `True` on overlay close and `:332` sets it `False` on overlay open. The run plan forbids exactly this at line 557: "`auto_collapsed` applies below 120 screen columns **without overwriting that request**", and KTD9 at lines 328-331 repeats it.

The result is that ordinary narrow-window use silently rewrites the operator's wide-window preference in both directions. Reproduced with the repository's own `InspectorHarness`:

- Start docked at 132 columns, narrow to 78, open and close the overlay with `ctrl+b` twice, widen back to 132 — the dock does **not** return, although the operator never collapsed it.
- Collapse the dock at 132, narrow to 78, peek at the overlay, widen back — the dock **does** return, although the operator explicitly closed it.

`tests/ui/test_inspector.py:196-218` exercises resize only and never opens the narrow overlay, which is why the suite is green over this.

**What to change.**

Give the overlay its own open and closed state and stop writing `requested_collapsed` from the `auto_collapsed` branch of `Inspector.toggle`: delete the assignment at `talaria/ui/inspector.py:329` and the one at `:332`, leaving that branch to flip `overlay_open` only. `is_docked` already reads `not auto_collapsed and not requested_collapsed`, so the wide-mode restore then depends solely on the operator's wide-mode choice.

**Verifiably resolved when.**

- A new test reproduces scenario one — docked at 132, narrow to 78, `ctrl+b` twice, widen to 132 — and asserts `app.inspector.is_docked` is true. It must fail on the current code.
- A second new test reproduces scenario two and asserts the panel stays collapsed after widening.
- The existing 119/120-column transition test still passes unchanged.
- A test asserts `requested_collapsed` is unchanged across a full narrow-overlay open-and-close cycle, in both starting states.
- `uv run pytest tests/ui/test_inspector.py tests/domain/test_changes.py -q` is green, then the full project check is green.

