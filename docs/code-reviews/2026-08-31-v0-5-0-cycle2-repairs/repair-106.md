# Repair brief — issue #106: true-bottom status bar (cycle 2)

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

## Requests (5)

| Request | Tier | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-08650f6d3036` | 3 | P3 | F-77 | `gated_auto` -> `review-fixer` | — |
| `fix-6de71f3e85b6` | 3 | P2 | F-58 | `manual` -> `review-fixer` | — |
| `fix-a9fd1a13c2a5 (part A)` | 3 | P2 | F-64 | `safe_auto` -> `review-fixer` | — |
| `fix-a9fd1a13c2a5 (part B)` | 3 | P2 | F-59 | `safe_auto` -> `review-fixer` | — |
| `fix-f3049112f842 (part A)` | 3 | P3 | F-73 | `safe_auto` -> `review-fixer` | — |

**Ordering — a hard three-step chain, and the most important instruction in this brief.** Findings F-77, F-58 and F-64 are one piece of code and its documentation, seen by three different lenses. Take them in this order and no other:

```
fix-08650f6d3036 (F-77: decide the contract)  -->  fix-6de71f3e85b6 (F-58: place the authority)  -->  fix-a9fd1a13c2a5 part A (F-64: correct the table)
```

F-77 decides **whether** the 20-to-31 column band pins `task_progress` to its minimum form at all. F-58 decides **where** that decision lives. F-64 states the outcome in `docs/terminal-ui.md`. Doing F-58 first relocates a special case that F-77 may delete; doing F-64 first documents a contract that has not been settled. If you take F-77's first option — remove the special case entirely — then F-58 collapses to a one-line deletion and F-64's row becomes a plain statement that the segment is dropped only below 20 columns. Say in the commit message which of F-77's two options you took, because the visual specification changes either way.

The remaining two requests, F-59 and F-73, are independent of that chain and of each other.

**Shared surface.** `fix-a9fd1a13c2a5 (part B)` edits `_inspector_model` in `talaria/ui/app.py`, which is issue #107's code. That is deliberate: the duplication was introduced on the status bar's side this round. Issue #107's brief has been told not to edit that method. Do not widen the edit beyond the extraction.

---

## `fix-08650f6d3036` — Tier 3 — P3 (F-77) — take first

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/status_bar.py`
- `docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`
- `tests/ui/test_status_bar.py`

**What is wrong.**

The cycle-1 repair for finding 41 moved the boundary at which `task_progress` disappears, but kept the shape of the defect. `talaria/ui/status_bar.py:486` pins the segment to its minimum form across the whole 20-to-31 band regardless of the room available. Measured by the accessibility lens: widths 20 through 31 all render ten cells; width 32 renders nineteen, and nineteen would fit inside twenty columns.

At width 31 the bar therefore renders ten of thirty-one available cells. Worse, `talaria/ui/status_bar.py:305-308` makes the minimum form's progress count and attention marker mutually exclusive, so when anything needs attention the operator sees a count with no denominator and cannot tell whether three of four tasks or three of thirty need them.

The visual specification sanctions this, so code and specification agree. The defect is in the contract, not in a divergence from it — which is why this is P3 and why the specification must change with the code.

**What to change.**

Either drop the special case and let the existing priority loop shrink the segment only when the row actually overflows, then amend the specification to say the segment keeps its compact form while it fits; or, if determinism across the band is the requirement, widen the minimum form to emit both the progress and the marker. Take one, name it in the commit message, and change the specification in the same commit.

**Verifiably resolved when.**

- A test walks widths 20 through 32 and asserts, at every width in the band, that the rendered cell count is within one cell of what the content requires — not merely that `task_progress` is present. Presence alone was true before this finding and is what the current walk asserts.
- A test asserts that at some width in the band, with a non-empty queue and non-zero attention, the operator can read both the progress count and the attention marker.
- `docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md` states the chosen rule, and the transition-walk test agrees with it.
- The full project check is green.

---

## `fix-6de71f3e85b6` — Tier 3 — P2 (F-58) — take second

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/status_bar.py`
- `tests/ui/test_status_bar.py`

**What is wrong.**

`_breakpoint` is the declared single authority mapping terminal width to a segment form and a drop set. After this round it no longer is. `talaria/ui/status_bar.py:414-421` returns identical tuples for the `>= 32` and `>= 20` branches, while the 20-to-31 band's actual behaviour lives in an inline conditional inside `render_status_bar` at `:485`, keyed on a hardcoded segment name.

At cycle 1 the two bands were distinguishable inside `_breakpoint` alone, because the `>= 20` branch's drop set included `task_progress`. The repair moved the distinction out of the authority and into the caller. A maintainer changing the narrow-terminal contract will read `_breakpoint`, see no distinction between the two bands, and edit the wrong place.

The architecture lens proved the inline expression carries real behaviour: replacing it with `initial_form` turns the complete breakpoint-walk test red.

**What to change.**

Depends on what F-77 decided. If F-77 removed the special case, delete the inline conditional and let this finding close with it. If F-77 kept a band-specific rule, widen `_breakpoint`'s return to carry per-segment form overrides, so the `>= 20` branch returns an explicit `task_progress` override and `render_status_bar` reads the override with no width literal of its own.

**Verifiably resolved when.**

- `grep -nE '\b(19|20|31|32)\b' talaria/ui/status_bar.py` shows width literals only inside `_breakpoint`.
- `grep -n 'task_progress' talaria/ui/status_bar.py` shows no occurrence inside `render_status_bar` guarded by a width comparison.
- The breakpoint-walk test is unchanged in its assertions and still passes, so the behaviour was relocated rather than altered — unless F-77 deliberately changed it, in which case say so.
- The full project check is green.

---

## `fix-a9fd1a13c2a5 (part A)` — Tier 3 — P2 (F-64) — take third

Route `safe_auto` -> owner `review-fixer`

This fix identifier covers four findings across four children. Part A is yours. Parts C and D belong to issues #107 and #109.

**Exact paths.**

- `docs/terminal-ui.md`

**What is wrong.**

`docs/terminal-ui.md:48` states `20-31 | Also drop task_progress; keep connection`. The code does not do that. `talaria/ui/status_bar.py:414-421` returns the identical drop set for the `>= 32` and `>= 20` branches; `task_progress` is dropped only below 20 columns. Measured with a populated view: widths 35, 31, 25 and 20 all render `task_progress` and `connection`; width 19 renders `connection` alone.

The responsive table is presented as the fixed product contract, and `docs/configuration.md` points operators at it as the authority on breakpoints. An acceptance tester verifying narrow-terminal behaviour expects the segment to be absent, finds it present, and reads a correct render as a defect.

**What to change.**

Change the 20-31 row to describe what F-77 settled on — as things stand, that `task_progress` is shortened to its attention count rather than dropped — and move the drop to the below-20 row where the code performs it. If F-77 removed the special case, the row says the segment is kept in full and dropped only below 20.

**Verifiably resolved when.**

- Every row of the band table in `docs/terminal-ui.md` matches a measured render at that width. The cheapest proof is a test that renders at one width per band and asserts the segment set against the table's own claim.
- The visual specification and `docs/terminal-ui.md` state the same rule.
- The full project check is green.

---

## `fix-a9fd1a13c2a5 (part B)` — Tier 3 — P2 (F-59)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/app.py`
- `tests/ui/test_inspector.py`
- `tests/ui/test_status_bar.py`

**What is wrong.**

The acceptance repair for the status bar rendering `agent: ?` during a live turn — commit e3c39fc, `fix(status): show observed live session model` — added a fleet roster lookup to `_status_agent` at `talaria/ui/app.py:1580-1600`. `_inspector_model` at `:1636-1645` already performed that lookup, calling `_status_agent` and then repeating the identical roster resolution. The repair copied rather than extracted.

Two methods in the same class now decide what model a session is on, and they can disagree: the inspector drops the provider prefix the status bar keeps. Nothing states which is intended. The architecture lens deleted `_inspector_model`'s duplicated block and all 750 tests under `tests/ui` stayed green, so the second copy is entirely unpinned.

The diff between the two reviewed revisions shows `_status_agent`'s branch replaced and `_inspector_model` untouched, so the duplication is a product of this round rather than pre-existing.

**What to change.**

Extract one private helper performing the session resolution and roster lookup once, and call it from both `_status_agent` and `_inspector_model`. If the inspector should genuinely omit the provider prefix, say so in its docstring and add a test fixing the difference deliberately.

**Verifiably resolved when.**

- Deleting the body of either caller's model resolution now turns a named test red — run that probe and say which test caught it. This is the assertion that failed to exist, and re-running the lens's own probe is the direct proof.
- A test pins the provider-prefix difference between the two surfaces in whichever direction is intended, so the two can no longer drift silently.
- `grep -c 'roster' talaria/ui/app.py` shows the lookup in one place.
- The full project check is green.

---

## `fix-f3049112f842 (part A)` — Tier 3 — P3 (F-73)

Route `safe_auto` -> owner `review-fixer`

This fix identifier covers two findings across two children. Part A is yours; part B belongs to issue #111.

**Exact paths.**

- `talaria/status/contract.py`
- `docs/configuration.md`

**What is wrong.**

`talaria/status/contract.py:311` appends a fixed string with no interpolation. An operator whose `status.segments` list contains four unrecognised names receives four byte-identical notices and cannot tell which entries were dropped — measured by the interface-contract lens with a six-entry list containing four unknown names.

Every sibling notice in the same module names its key or value, including the duplicate branch on the very next line at `:314`, which does interpolate the entry. The one notice that most needs the offending name is the only one without it.

**What to change.**

Interpolate a defanged form of the entry, reusing the presentation defang helper, so the notice names what was rejected. Note that `talaria/status/` is framework-free (ADR-0002) — if the defang helper lives in the presentation layer, write or move a local one rather than importing upward.

**Verifiably resolved when.**

- A test supplies two distinct unknown segment names and asserts the two notices differ from each other and each contains its own name.
- A test supplies an unknown segment name containing an escape byte and asserts the notice carries the defanged form, not the raw byte.
- `python -c "import talaria.status.contract, sys; assert 'textual' not in sys.modules"` still succeeds.
- The full project check is green.
