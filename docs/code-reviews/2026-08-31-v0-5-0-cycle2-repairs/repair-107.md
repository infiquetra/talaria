# Repair brief — issue #107: right inspector (cycle 2)

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
| `fix-8d8697f15041` | 3 | P2 | F-50 | `gated_auto` -> `review-fixer` | — |
| `fix-a9fd1a13c2a5 (part C)` | 3 | P2 | F-62 | `safe_auto` -> `review-fixer` | — |

**Shared surface, not an ordering dependency.** Issue #106 holds a request (`fix-a9fd1a13c2a5 (part B)`, finding F-59) that extracts a helper and edits `_inspector_model` in `talaria/ui/app.py`. That is inspector-owned code being changed from the status bar's brief, because the status bar is where the duplication was introduced this round. Do not edit `_inspector_model` from this brief; if you need to, coordinate rather than both writing it.

---

## `fix-8d8697f15041` — Tier 3 — P2 (F-50)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/inspector.py`
- `tests/ui/test_inspector.py`

**What is wrong.**

`Inspector.set_terminal_width` at `talaria/ui/inspector.py:313-323` closes an open narrow overlay by assigning `overlay_open = False` directly, at `:320` and again at `:322`. It never calls `_close_overlay`, which is the only method that clears and re-focuses `_previous_focus`. Every other overlay-close path in the module does restore focus.

`talaria/ui/app.py:1841` routes every terminal resize through `set_terminal_width`, so this fires on ordinary window resizing. Measured by the correctness lens on the real application at 132x30 columns: `ctrl+b`, resize to 78, `ctrl+b`, resize back to 132 leaves `app.focused is None` and `inspector.display` `False`. Typing `a b c` at that point leaves the composer empty; one `Tab` lands on the transcript. The operator's keystrokes go nowhere and nothing tells them why.

The lens also ran the control: against the pre-repair code, focus stayed on the then-visible inspector and the composer still received nothing — so a focus gap in this area predates the repair. `app.focused is None` does not. The cycle-1 repair to the wide-dock preference changed the shape of the gap next door.

**What to change.**

Replace the two bare assignments with a guarded call to `_close_overlay(restore_focus=True)` before `_sync_geometry`. Keep the `was_auto_collapsed` branch, so a narrow-to-narrower resize leaves an already-open overlay open.

**Verifiably resolved when.**

- A test drives the exact measured sequence — open at wide, narrow, open, widen — and asserts `app.focused is not None` **and** that the widget holding focus is the one that held it before the overlay opened. Asserting only non-null passes if focus lands anywhere at all.
- A second test asserts the composer receives typed characters after that sequence, which is the operator-visible symptom.
- A third test asserts a narrow-to-narrower resize does **not** close an open overlay, so the `was_auto_collapsed` branch is pinned and the repair does not overshoot.
- `uv run pytest tests/ui/test_inspector.py -q` is green, then the full project check is green.

---

## `fix-a9fd1a13c2a5 (part C)` — Tier 3 — P2 (F-62)

Route `safe_auto` -> owner `review-fixer`

This fix identifier covers four findings across four children. Part C is yours. The other parts belong to issues #106 and #109 and are not your work.

**Exact paths.**

- `talaria/ui/inspector.py`
- `docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`
- `docs/terminal-ui.md`
- `tests/ui/test_inspector.py`

**What is wrong.**

`talaria/ui/inspector.py:30` defines a 33-cell empty-state sentence. `:166-170` sets the row to height 1, `text-wrap: nowrap`, `text-overflow: ellipsis`, inside a bordered panel that leaves 32 usable cells at the default docked width of 36 columns.

Measured on a live application: at panel width 36 the operator reads `[none available from this ses...`; at the minimum 28 it is shorter still; the full sentence appears only at 48. The row has no wrap and no horizontal scroll, so the tail is unreachable by any means available to the operator.

The visual specification and `docs/terminal-ui.md` both quote the full string. Checklist item 17 requires that sentence to appear and was recorded as a pass — the truncated form is visible in the preserved capture under `superseded/driver-pinned-dimensions/`. An empty state cut mid-word does not tell a first-time operator whether the section is empty, still loading, or broken, which is the exact distinction the sentence exists to draw.

**What to change.**

Either let the empty row wrap — change its height to `auto` and its wrap to `wrap`, so the sentence occupies two rows at 28 and 36 columns — or shorten the constant to fit 30 cells and update the visual specification, `docs/terminal-ui.md` and the checklist item in the same commit.

**Verifiably resolved when.**

- A test renders the empty state at panel widths 28, 36 and 48 and asserts the complete sentence is present in the rendered output at **all three**. A width-48 assertion alone is what currently passes.
- Whichever route is taken, the string in `talaria/ui/inspector.py:30`, the visual specification, and `docs/terminal-ui.md` are byte-identical to each other.
- `uv run pytest tests/ui/test_inspector.py -q` is green, then the full project check is green.
