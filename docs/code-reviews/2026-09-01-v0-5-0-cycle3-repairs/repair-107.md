# Repair brief — issue #107: right inspector (cycle 3)

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
| `fix-eaa556eca29a` | P2 | F-102 | `safe_auto` -> `review-fixer` | **YES — invalidates the receipts** | — |
| `F-85 (carried — not in the sixteen)` | P3 | F-85 | `safe_auto` -> `review-fixer` | yes, but not in the round | — |

**Shared surface.** Issue #106's `fix-c5fa2bba78b8` also edits `talaria/ui/app.py`, in a different region — the model-identity helper at `:1573` against your region map at `:6374`. Coordinate rather than both writing the file. This is the same file two lanes shared in the cycle-2 round.

---

## `fix-eaa556eca29a` — P2 (F-102)

Route `safe_auto` -> owner `review-fixer`. **Touches `talaria/ui/app.py`.**

**Exact paths.** `talaria/ui/app.py`, `tests/ui/test_b1_discard_notice.py`

**What is wrong.**

The application already has a rule for keys that reach a region accepting no text: it writes `press tab to return to the message box — typing is paused while the <region> holds the focus`. `talaria/ui/focus.py`'s own module docstring names the failure it prevents — "the operator types a message, sees no characters appear, and has nothing on screen to tell them why."

The inspector is missing from that map. `talaria/ui/app.py:6374-6378` lists only `transcript`, `agents` and `prompts`, and `:6383-6396` carries notices for the same three. The inspector is a surface this release added, it is focusable, it is reachable by tab, and it accepts no text — so it is the one focusable no-text surface where the rule does not apply.

Measured on a live application at 140x30: with the transcript focused, five printable keys leave the composer empty and produce the transcript notice, and `_no_text_region()` returns `'transcript'`. With the **inspector** focused, the same five keys leave the composer empty, the notice is unchanged at `replay — not connected`, the string `typing is paused` is absent from the exported screen, and `_no_text_region()` returns `None`.

**The overlay case is the worse of the two, because the operator did not choose that focus.** Measured at 100x30: pressing `ctrl+b` from the composer opens the narrow overlay and the application moves focus to the Inspector by itself — `talaria/ui/inspector.py:340` calls `self.call_after_refresh(self._focus_first_row)`. The status row reads `caret: inspector`. Typing five characters then leaves the composer empty with no notice. The operator pressed a key to *look* at something, the application took the caret, and their next sentence vanishes silently.

For contrast, the theme picker also discards printable keys, but carries a permanent header naming the keys it accepts, so an operator there is not stranded. The inspector carries no such row.

**What to change.**

Add `inspector` to `TalariaApp._NO_TEXT_REGION_IDS` and a matching sentence to `_DISCARD_NOTICE_BY_REGION`, in the same house register as the existing three: `press tab to return to the message box — typing is paused while the inspector holds the focus`. The existing latch and focus-change clearing logic then apply unchanged.

**Verifiably resolved when.**

- A test focuses the inspector, presses a printable key, and asserts the notice appears.
- A second test **opens the narrow overlay with `ctrl+b` from the composer**, types, and asserts the same. This is the path where the application takes focus without being asked, and a test that focuses the inspector directly does not exercise it.
- A test asserts the notice clears on focus change, so the latch behaves as it does for the other three regions.
- The full project check is green.

**Acceptance consequence.** `talaria/ui/app.py` ships in the wheel. Landing this forces a re-drive; five other requests are on the same path and the coordinator is cutting one candidate for all six.

---

## `F-85 (carried — not in the sixteen)` — P3 (F-85)

Route `safe_auto` -> owner `review-fixer`. **Not part of the approved round** — pre-existing.

**Exact paths.** `talaria/ui/inspector.py`

**What is wrong.**

`talaria/ui/inspector.py:322-327` holds an `if`/`elif` pair whose two bodies are byte-identical text — both read `if self.overlay_open: self._close_overlay(restore_focus=True)`. The condition structure now carries no information: the only case the pair excludes is "was already narrow and stayed narrow", which is the rule the code is actually expressing.

The cycle-2 overlay focus-restore repair created this by replacing two identical assignments with two identical calls. Read against `83ffd27a`, the same pair previously held two copies of `self.overlay_open = False`, so the shape predates the cycle and the repair preserved rather than collapsed it.

No behaviour is wrong today. The concern is that this is the specific shape that invites a later editor to change one branch and not the other — in a method whose correctness is about which resize transitions close an open overlay, in a file the cycle-2 dispatch manifest flagged as edited by more than one lane.

**What to change, if the coordinator scopes it in.**

Collapse the pair to the rule it expresses: close the overlay when it is open, unless the widget was already auto-collapsed and stayed auto-collapsed. One conditional with a comment naming the excluded case.

**Verifiably resolved when.**

- The existing overlay-close tests pass unchanged, so the collapse is a refactor and not a behaviour change.
- A test asserts a narrow-to-narrower resize leaves an open overlay open — the case the excluded branch is about.
