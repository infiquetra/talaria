# Repair brief — issue #106: true-bottom status bar

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (5)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-be74fdf7fea8` | P2 | F-23 | `manual` -> `downstream-resolver` | — |
| `fix-f382e9cc6103` | P2 | F-22 | `manual` -> `review-fixer` | — |
| `fix-ea3a40a2682e` | P3 | F-40 | `manual` -> `review-fixer` | — |
| `F-26 (advisory — no fix request)` | P2 | F-26 | `advisory` -> `human` | — |
| `F-41 (advisory — no fix request)` | P3 | F-41 | `advisory` -> `human` | — |

---

## `fix-be74fdf7fea8` — P2 (F-23)

Route `manual` -> owner `downstream-resolver`

**Exact paths.**

- `talaria/status/local.py`
- `talaria/ui/status_bar.py`
- `talaria/ui/app.py`
- `tests/ui/test_status_bar.py`

**What is wrong.**

`capture_local_status` at `talaria/ui/status_bar.py:155` is pure local-environment capture with no presentation content — it runs a Git subprocess at `:168` to read the branch — but it sits in a module that imports `rich.text` and `textual.widgets.Static` at `:20-22`. Nothing can reuse it without pulling in the terminal framework, including `talaria/status/`, which is the framework-free package the repository already designates for status data, and the headless measurement path in `talaria/replay/gate.py`.

The repository already owns a hardened process-execution seam at `talaria/status/runner.py:183`; this adds a second, unrelated synchronous discipline in the presentation layer. `talaria/ui/` is also exactly the tree the ADR-0002 import sweep exempts, so further input-output accreting here has no mechanical brake.

The one-shot capture itself is correct and sanctioned by the run plan ("Capture cwd and Git branch once at launch from local process state, never per render"). The concern is placement and reuse, not latency: the call happens in `TalariaApp.__init__` before the event loop.

**What to change.**

Move `LocalStatus` (`talaria/ui/status_bar.py:63-70`) and `capture_local_status` (`:155-179`) into a new framework-free `talaria/status/local.py`, and import them from `talaria/ui/status_bar.py` and `talaria/ui/app.py`. Leave the segment renderers where they are.

**Verifiably resolved when.**

- `python -c "import talaria.status.local, sys; assert 'textual' not in sys.modules"` succeeds.
- `grep -n 'subprocess' talaria/ui/status_bar.py` returns nothing.
- The existing status-bar tests pass unchanged, and a new test exercises `capture_local_status` without importing `talaria.ui`.
- The full project check is green.

---

## `fix-f382e9cc6103` — P2 (F-22)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/needs_you.py`
- `docs/engineering-journal/DECISIONS.md`

**What is wrong.**

This release removed the needs-you row from compose, but the widget that drew it survives at `talaria/ui/needs_you.py:211` with a docstring at `:214` asserting "Composed at first mount and never unmounted, which is the whole of its geometry contract" and a citation at `:224` to a test name that no longer exists. There is no instantiation and no mount anywhere in `talaria/` or `tests/`.

Worse, the orphaned class is the only remaining consumer of `talaria.domain.queue.summary_line` in the application, so the domain's canonical queue-summary formatter now feeds nothing that renders. `talaria/ui/status_bar.py:330-355` builds the `task_progress` text from its own parts, never from `summary_line`, while KTD5 in the decision journal says the needs-you summary becomes the `task_progress` segment. Nothing tests the orphan, so the two forms can drift with nothing failing.

**What to change.**

Delete `NeedsYouBar` (`talaria/ui/needs_you.py:211-265`) and its `__all__` entry at `:382`, and fix the module docstring at `:16` that still points at `summary_line` as the row's source. If the class is being kept deliberately for a future surface, replace the composition claim and the dead test citation with a sentence saying it is currently unmounted and naming what would mount it. Either way, record in `DECISIONS.md` that `task_progress` computes its own completed-of-total form rather than reusing `summary_line`, so KTD5 stops describing code that does not exist.

**Verifiably resolved when.**

- `grep -rn 'NeedsYouBar' talaria/ tests/` returns either nothing, or only a definition whose docstring no longer claims it composes and no longer cites a non-existent test.
- The KTD5 entry in `DECISIONS.md` describes what the code does.
- `uv run pytest tests/ui/test_needs_you.py -q` is green, then the full project check is green.

---

## `fix-ea3a40a2682e` — P3 (F-40)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `tests/ui/test_status_bar.py`

**What is wrong.**

`tests/ui/test_status_bar.py:315` is named `test_missing_context_and_model_facts_render_unknown_instead_of_triggering_a_fetch`, but its body at `:315-330` only compares a rendered string. Nothing observes whether a fetch was attempted. If a future status renderer started reaching for the missing model facts over the wire while still rendering the placeholder, the test would stay green and its name would be a false claim in the suite. The suite already has the right tool: `tests/ui/conftest.py:81` defines `RecordingDispatcher`, and `tests/ui/test_diff_viewer.py:408` uses `assert dispatcher.operator_calls == []` as a real no-dispatch assertion.

**What to change.**

Rewrite the test around the running app with a `RecordingDispatcher`, mount the bar with the missing facts, and assert both the `?` placeholders and `dispatcher.operator_calls == []`. If a no-dispatch assertion is genuinely out of reach for this pure renderer, rename the test to drop the unasserted claim.

**Verifiably resolved when.**

- Either the test asserts an empty dispatcher call list, or its name no longer promises one.
- `uv run pytest tests/ui/test_status_bar.py -q` is green, then the full project check is green.

---

## `F-26 (advisory — no fix request)` — P2 (F-26)

Route `advisory` -> owner `human`

**Exact paths.**

- `docs/plans/2026-08-30-talaria-v0-5-0-run-plan.md`

**What is wrong.**

The run plan declares its consolidated table "the only schema ledger for the run" and specifies at line 169 that `status.segments` falls back to "the full default if no known row remains". The shipped code at `talaria/status/contract.py:319-321` renders `connection` alone, and `docs/configuration.md:79` agrees with the code. Both behaviours are defensible, but only one is written down as normative and the plan carries no amendment recording the change, so anyone implementing against the ledger encodes the wrong fallback.

This is **advisory**: it produces no code change unless the coordinator decides the plan was right.

**What to change.**

Amend the `status.segments` row in the plan to say the connection row alone is the floor, with a one-line reason. If the plan is right instead, change `talaria/status/contract.py:319-321` to return `DEFAULT_STATUS_SEGMENTS` and update `docs/configuration.md:79` in the same commit.

**Verifiably resolved when.**

- The plan ledger, `talaria/status/contract.py` and `docs/configuration.md` all state the same fallback.
- If the code changes, the existing fallback tests are updated and still carry a valid-value counterexample.

---

## `F-41 (advisory — no fix request)` — P3 (F-41)

Route `advisory` -> owner `human`

**Exact paths.**

- `talaria/ui/status_bar.py`
- `docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`
- `tests/ui/test_status_bar.py`

**What is wrong.**

In a terminal 20 to 31 columns wide the bottom bar renders only `[~] retry`, nine cells in a thirty-one column row, and the `!N` marker saying how many items need the operator has already been discarded. `talaria/ui/status_bar.py:455-458` returns a drop set containing `task_progress` unconditionally for that band, before the fit-based degradation loop ever runs. Measured: width 31 gives nine cells, width 32 gives twenty-one cells including the attention marker, so the drop is not forced by space.

The code matches the visual specification at `:497`; the tension is with the specification's own step-5 rule at `:522` that a segment drops only after every lower-priority segment reaches its minimum form. Impact is bounded because prompt cards still render in the body, so this is loss of a redundant signal — hence P3 and advisory.

**What to change.**

If the coordinator decides the fixed bands are a starting form rather than a hard erasure: render `task_progress` in its minimum form (already just `!N` when attention is non-zero, per `talaria/ui/status_bar.py:353-355`) and let the existing width loop remove it only on real overflow. This changes normatively specified behaviour, so the specification at `:497` and the transition-walk tests must change in the same commit.

**Verifiably resolved when.**

- If taken: the specification, the code and the eighteen-width transition walk all agree, and a test asserts the attention marker survives at width 31 when the queue is non-empty.
- If not taken: a one-line note in the specification records that the band deliberately pre-drops `task_progress` ahead of the fit rule, so the two rules stop appearing to contradict.

