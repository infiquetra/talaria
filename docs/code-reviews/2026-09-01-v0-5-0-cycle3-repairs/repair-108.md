# Repair brief — issue #108: read-only diff viewer (cycle 3)

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

## Requests (1 approved, 1 carried)

| Request | Severity | Findings | Route | Touches `talaria/`? | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-5fb6e79d5a8f` **— repair-induced regression** | P2 | F-86 | `gated_auto` -> `review-fixer` | **YES — invalidates the receipts** | — |
| `F-87 (carried — not in the sixteen)` | P3 | F-87 | `manual` -> `review-fixer` | yes, but not in the round | — |

**Hard ordering, and it reaches outside this brief.** `fix-5fb6e79d5a8f` may change which keys reach the pickers, which rewrites the keybinding table in `docs/terminal-ui.md`. Issue #106's `fix-019217aad2ef` rewrites the width-band table in that same file. Land yours first, or resolve one conflict deliberately.

---

## `fix-5fb6e79d5a8f` — P2 (F-86) — **A REPAIR-INDUCED REGRESSION. READ THIS SECTION BEFORE THE CODE.**

Route `gated_auto` -> owner `review-fixer`. **Touches `talaria/domain/commands.py` — this invalidates the 43 acceptance receipts.**

**You are undoing your own prior repair, not making a fresh change. This is the third cycle on one keyboard collision.**

The history matters more than the diff:

- **Cycle 1, finding 33.** F6 and F7 were registered as priority bindings and shadowed the composer's select-all. The repair dropped `priority=True`.
- **Cycle 2, finding F-49.** That inverted the collision rather than removing it. Textual 8.2.8's `TextArea.BINDINGS` maps `f6` to `select_line` and `f7` to `select_all`, and the composer holds focus at launch — so F6 and F7 became dead from the state the application actually starts in. The review's own cycle-1 acceptance criterion was at fault: it asked for a test proving the keys still reach the pickers *when focus is elsewhere*, and the landed test was named exactly that, focused the transcript, and passed while the keys were dead.
- **Cycle 3, this request.** The cycle-2 repair did not make F6 and F7 work from launch focus. It added **F11 and F12** as new collision-free keys and left F6 and F7 dead there. That is a defensible design choice and the composer half of the criterion holds cleanly. But `docs/terminal-ui.md` was corrected and the running application's own command list was not.

**What is wrong now.**

`talaria/domain/commands.py:422` renders as `Open the model picker, select a row, or set a row as the profile's default (F6)`, and `:437` ends `(F7)`. Neither row mentions F11 or F12, and **no other surface inside the running application names them** — the help bar at `talaria/ui/app.py:1092` lists no picker key in either mode.

Measured on the production application at 132x30 through Textual's test pilot, with the launch focus untouched: `f6` calls `open_picker` zero times and selects the whole composer line; `f7` does the same and selects the whole document; `f11` calls `open_picker('models')` exactly once and `f12` calls `open_picker('profiles')` exactly once, both leaving a live composer selection unchanged.

`docs/terminal-ui.md:136-137` now says `/models or F11; F6 only outside composer focus`. So the two places that state one contract disagree, and the one an operator reaches without leaving the terminal is the wrong one. At the diff base that help string was **true** — F6 was a priority binding and reached the picker from launch focus, measured directly. This release made it false.

**Aim at the contract, not a third key swap.**

Do not simply edit the two strings and stop, and do not move the keys again without deciding why. Settle three questions and make every statement agree:

1. Which keys reach the pickers **from every focus, including launch focus**? F11 and F12 do today.
2. What, if anything, do F6 and F7 promise? Today they are Textual's text-area keys inside the composer and picker aliases outside it. That is defensible if it is stated; it is a trap if it is not.
3. Where is that contract stated? Three places today: `talaria/domain/commands.py`, `docs/terminal-ui.md:136-137`, and the code comment at `talaria/ui/app.py` calling the aliases symmetrical.

**Verifiably resolved when.**

- A test **reads the rendered command-list rows and the documentation keybinding table and asserts they name the same keys.** The drift between two statements of one contract is what let this survive two repairs; a test that checks only one of them repeats the cycle-1 mistake exactly.
- A test starts the application, makes **no focus change at all**, presses each key the command list names, and asserts the named surface opens. Launch focus is the case that matters.
- A test presses F6 and F7 with the composer focused and containing text, and asserts the composer's selection behaves as the settled contract says — so whichever way you decide, cycle-1 finding 33 does not reopen.
- The code comment at `talaria/ui/app.py` no longer claims a symmetry the bindings do not have.
- The full project check is green.

**Acceptance consequence.** `talaria/domain/commands.py` ships in the wheel. Landing this forces a re-drive. Coordinate with the other five product-code requests so the coordinator cuts one candidate, not two.

---

## `F-87 (carried — not in the sixteen)` — P3 (F-87)

Route `manual` -> owner `review-fixer`. **Not part of the approved round** — pre-existing, so the engine excluded it from consolidation. Recorded so it is not lost.

**Exact paths.** `talaria/ui/app.py`, `tests/ui/test_picker.py`

**What is wrong.**

`talaria/ui/app.py:4043` calls `push_screen(PickerDialog(source), ...)` unconditionally, and nothing in `open_picker` inspects the screen stack. F11 is a priority binding, so it reaches the application even while the modal picker owns the screen. Measured: pressing `f11` three times leaves the screen stack `['Screen', 'PickerDialog', 'PickerDialog', 'PickerDialog']`, and one Escape pops one dialog — so an operator who double-presses sees a dialog that appears to refuse to close.

The method is named `action_toggle_picker` and never toggles.

**The archaeology is worth knowing, because it bears on the request above.** The correctness lens established by probing the diff base that this gap predates the release: at `5efa19cc`, F6 was a priority binding and stacked identically. Cycle 2's repair removed `priority=True` from F6 and F7, which **incidentally closed this hole**. Cycle 3's repair reopened it by introducing two fresh priority bindings on F11 and F12. So whichever key space `fix-5fb6e79d5a8f` settles on, a priority binding on a picker key reopens this unless `open_picker` guards the stack.

**What to change, if the coordinator scopes it in.**

Make the action match its name: when the active screen is already a `PickerDialog`, dismiss it instead of pushing another. The narrower alternative is an early return in `open_picker`.

**Verifiably resolved when.**

- A test presses the picker key twice and asserts the screen stack never holds more than one dialog. The current tests only ever press it once.
- The same assertion holds for whichever keys `fix-5fb6e79d5a8f` settles on.
