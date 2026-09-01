# Repair brief — issue #109: interaction and readability polish (cycle 2)

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

## Requests (3)

| Request | Tier | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-a9fd1a13c2a5 (part D)` | **1** | P2 | F-61 | `safe_auto` -> `review-fixer` | — |
| `fix-5f5e651ea6c1` | 3 | P3 | F-76 | `safe_auto` -> `review-fixer` | — |
| `F-78 (advisory — no fix request)` | 3 | P3 | F-78 | `advisory` -> `review-fixer` | — |

**Why the first one matters more than its P2 label suggests.** `fix-a9fd1a13c2a5 (part D)` is the **only** request in the entire cycle-2 set that sits on `keyboard-focus`, the accessibility dimension scoring 6.0 against a 7.0 floor — the second-lowest dimension in the whole review. No other child can move it. If this request is skipped, the accessibility lens fails its floor in cycle 3 regardless of what else lands.

**One coordination note.** The advisory F-78 and issue #110's `fix-e88e3801ea3e` are two halves of one hazard: the `TEXTUAL_ANIMATIONS` environment variable. Decide F-78 before #110 writes its regression test, so that test asserts a framework default the application then actually honours. This is a soft dependency — both repairs are independently correct — but deciding in the other order wastes a test.

---

## `fix-a9fd1a13c2a5 (part D)` — Tier 1 — P2 (F-61)

Route `safe_auto` -> owner `review-fixer`

Sub-floor dimension: `keyboard-focus` = **6.0** against a floor of 7.0. This request is the sole lever on it.

This fix identifier covers four findings across four children. Part D is yours. Parts A and B belong to issue #106, part C to issue #107.

**Exact paths.**

- `talaria/ui/transcript.py`
- `talaria/ui/app.py`
- `tests/ui/test_transcript_bounds.py`

**What is wrong.**

The acceptance repair for the unpinned transcript jumping after later updates — commit 445d243, `fix(polish): preserve transcript spacing and scroll anchors` — fixed the pointer path and not the keyboard path.

`talaria/ui/app.py:6641` unpins follow mode for `pageup` and `home` only. `talaria/ui/transcript.py:1984` handles `MouseScrollUp`. There is no key handler on the pane that unpins for the arrow keys. Measured by the accessibility lens on a live application at 100x20: with the transcript focused, four `up` presses moved `scroll_y` from 9.0 to 5.0 with `follow` still `True`; `pageup` and `home` both gave `follow` `False`.

`talaria/ui/transcript.py:1917-1924` — `restore_reading_anchor` branches on `follow` and calls `scroll_end`. So a keyboard-only operator who scrolls up with the arrow keys keeps `follow` true, and the next update scrolls back to the end, discarding the line they were reading. The same reading task succeeds with a pointer and fails without one. Sixty-eight focus, inspector and function-key tests pass, so nothing covers the keyboard scroll path.

**What to change.**

Give `TranscriptPane` its own key handling for the scroll keys, so the unpin happens wherever the key is consumed — mirroring `on_mouse_scroll_up` — rather than extending the app-level key list. The app-level list is where the two working keys already live and is why the gap was invisible.

**Verifiably resolved when.**

- A test focuses the transcript, presses `up`, and asserts `follow` is `False` **and** that a subsequent transcript update leaves `scroll_y` where the operator left it. The second half is the operator-visible symptom; asserting the flag alone re-tests the flag rather than the behaviour.
- The same two assertions for `down` and for any other scroll key the pane consumes.
- A test asserts the pointer path still unpins, so the repair does not move the handling and break what already worked.
- The full project check is green.

---

## `fix-5f5e651ea6c1` — Tier 3 — P3 (F-76)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `tests/test_framework_boundaries.py`

**What is wrong.**

The cycle-1 repair extended the ADR-0002 framework-free guarantee to three further packages, and it works. But the sweep at `tests/test_framework_boundaries.py:22-33` enumerates **packages**, and `talaria/config.py` is a top-level module, so nothing enumerates it.

This round is what made that matter: `talaria/config.py:50-52` now imports from the `themes` package, an edge added this round, which puts `config.py` squarely in the framework-free layer it is not guarded as. The architecture lens appended `import textual` to `talaria/config.py` and sixty tests across four relevant files stayed green. The counter-probe confirms the module is framework-free in fact — a fresh interpreter importing `talaria.config` reports `textual` absent — so this is a missing guard, not a live violation.

**What to change.**

Generalise the sweep helper to accept an explicit module-name list and add a fourth parametrisation covering `talaria.config`.

**Verifiably resolved when.**

- Re-run the lens's probe: append `import textual` to `talaria/config.py`, confirm the sweep turns red, and remove it. Report which test caught it. A green run is the current state.
- The parametrisation names the module explicitly, so a future top-level module is a one-line addition rather than a silent gap.
- The full project check is green.

---

## `F-78 (advisory — no fix request)` — Tier 3 — P3 (F-78)

Route `advisory` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/app.py`
- `docs/configuration.md`
- `docs/terminal-ui.md`
- `tests/ui/test_motion.py`

**What is wrong.**

`docs/configuration.md:76` states that the reduced-motion setting has no environment or command-line alias. `talaria/ui/app.py:1208-1209` builds the motion policy from the configuration flag alone, while the animation level's false branch keeps whatever the environment supplied — and Textual's application constructor takes its animation level from `TEXTUAL_ANIMATIONS`.

So an operator who reaches for the standard framework variable gets a state Talaria never describes: key-driven scrolling becomes immediate while decorative spinner frames keep cycling. Measured by the accessibility lens: running the motion test suite with the variable exported fails on the animation level, where the unchanged run passes. An operator with a vestibular trigger who sets the variable they already know will believe they have turned motion off and will still see animation.

This is **advisory**: it produces no code change unless the coordinator decides which of the two readings is intended. Both are defensible.

**What to change.**

If Talaria's own flag should be the sole authority, assign the full animation level explicitly on the false branch, so the sentence at `docs/configuration.md:76` becomes true on every machine. If the variable should instead be honoured, derive the flag from it before building the policy and document the alias in both guides.

**Verifiably resolved when.**

- Whichever reading is taken, `uv run pytest tests/ui/test_motion.py -q` passes **both** with `TEXTUAL_ANIMATIONS` unset and with it exported as `none`, and the assertions differ between the two runs only if the alias is being honoured deliberately.
- `docs/configuration.md:76` states the behaviour the code implements.
- Tell issue #110 which reading you took, so its regression test for `fix-e88e3801ea3e` asserts the right default.
