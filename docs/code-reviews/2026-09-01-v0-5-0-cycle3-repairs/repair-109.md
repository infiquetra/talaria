# Repair brief — issue #109: interaction and readability polish (cycle 3)

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

## Requests (1 approved)

| Request | Severity | Findings | Route | Touches `talaria/`? | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-70e7c4a5b4cc` **— repair-induced regression** | P2 | F-101 | `gated_auto` -> `review-fixer` | **YES — invalidates the receipts** | — |

One request, and it is a regression your own lane introduced. There is no ordering dependency and no shared surface: `talaria/ui/transcript.py` is yours alone this round.

---

## `fix-70e7c4a5b4cc` — P2 (F-101) — **A REPAIR-INDUCED REGRESSION. READ THIS SECTION BEFORE THE CODE.**

Route `gated_auto` -> owner `review-fixer`. **Touches `talaria/ui/transcript.py` — this invalidates the 43 acceptance receipts.**

**You are undoing your own prior repair, not making a fresh change.**

**What the earlier repair fixed, and it did fix it.** Cycle-2 finding F-61: a keyboard-only operator who focused the transcript and scrolled up with the arrow keys kept `follow` true, so the next update called `restore_reading_anchor`, saw `follow`, and scrolled back to the end — discarding the line they were reading. The mouse wheel, Page Up and Home all unpinned correctly, so the same reading task succeeded with a pointer and failed without one.

That repair works, and the accessibility lens proved it by mutation rather than by reading: on a live application at 100x20 it focused the transcript, pressed `up` six times, fed a streaming update, and the pane still sat at `scroll_y` 19.0 with the operator's row on screen. Deleting `TranscriptPane.on_key` on a disposable copy and re-running the identical probe returned the old behaviour — `follow` stayed True, `scroll_y` jumped 19.0 to 26.0, the row left the screen. **Do not lose that.**

**What it broke.**

The same handler unpins on `down` and `pagedown` as well, including when the pane is already at the exact bottom. Measured on a live application at 100x20 with `follow_bottom()` called, so `scroll_y == max_scroll_y == 25.0` and `follow` is True:

- Press `down`: `follow` becomes False, `scroll_y` stays 25.0, the composer notice is unchanged. **Nothing moves and nothing is said.**
- Feed one streaming turn: `follow` still False, `scroll_y` still 25.0, `max_scroll_y` grows to 26.0, and the new line is absent from the exported screen.
- Identical for `pagedown`.

So a keyboard-only operator who presses the natural "show me more" key at the bottom of a live turn silently stops following, and every later token lands off-screen. The session looks like it went quiet. Recovery exists — `end` restores follow, measured — but nothing on screen says the state changed or that a recovery key exists.

`TranscriptPane` only unpins on `MouseScrollUp`, so a pointer user cannot reach this state by scrolling down at the bottom. **The gesture is safe with a pointer and unsafe without one, which is the exact inversion of the complaint F-61 was raised on.**

**Why the test did not catch it.** `tests/ui/test_transcript_bounds.py:186-225` parametrises `down` and `pagedown`, but calls `pane.scroll_to(y=max(1, max_scroll_y // 2))` for those two keys first. It never visits the at-bottom case. This is the pattern the architecture lens named this cycle: a fixture constructed beside the assertion, inside the subset the implementation already handles.

**Aim at the unpin contract, not the key list.**

The rule the handler is reaching for is *unpin when the reading position moves away from the bottom*. Express that: call `hold_anchor()` for `up`, `pageup` and `home` unconditionally, and for `down` and `pagedown` only when the pane is not already at `max_scroll_y` — or only when `scroll_y` actually changed after the key was handled. Do not extend the app-level key list; the app-level list is where the two working keys already lived and is why the gap was invisible.

Separately worth raising with the coordinator, not repairing here: a non-following transcript has no on-screen signal at all. That is a design question, not this fix.

**Verifiably resolved when.**

- A test focuses the pane, calls `follow_bottom()`, presses `down` **at the exact bottom**, and asserts `follow` stays True **and** a subsequent update still shows the newest line. Both halves — the flag alone re-tests the flag.
- The F-61 direction still holds under its own falsification: focus, press `up`, feed an update, assert `scroll_y` is where the operator left it and the row is still on screen. Run the accessibility lens's mutation probe as your proof — delete `TranscriptPane.on_key`, watch that test go red, restore.
- The pointer path still unpins on scroll-up.
- The full project check is green.

**Acceptance consequence.** `talaria/ui/transcript.py` ships in the wheel. Landing this forces a re-drive. Five other requests also touch product code; the coordinator is cutting one candidate for all six.
