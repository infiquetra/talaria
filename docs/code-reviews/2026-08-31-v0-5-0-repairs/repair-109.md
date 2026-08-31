# Repair brief — issue #109: interaction and readability polish

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (2)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-d5fd7c121ecf` | P1 | F-1 | `safe_auto` -> `review-fixer` | — |
| `fix-33e4c93833a2` | P2 | F-20 | `manual` -> `review-fixer` | talaria-w1 |

---

## `fix-d5fd7c121ecf` — P1 (F-1)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/app.py`
- `tests/ui/test_motion.py`
- `docs/terminal-ui.md`

**What is wrong.**

**This is the release's one confirmed accessibility failure, and the setting fails at exactly the thing it exists to do.**

An operator with vestibular sensitivity sets `ui.reduced_motion = true` and still receives eased, animated scrolling on every arrow key, PageUp, PageDown, Home and End press in the transcript, inspector, prompt region, command palette and diff canvas. Key-driven scrolling is the dominant motion surface in this interface.

`MotionPolicy` only reaches the scrolls Talaria itself initiates. No Talaria widget defines scroll bindings — `talaria/ui/transcript.py` declares no `BINDINGS` — so operator keypresses fall through to Textual's `Widget.action_scroll_end`, `action_page_down` and friends, whose `animate` parameter defaults to `True` and which are gated only on `App.animation_level`. Talaria never assigns `animation_level`; a grep across `talaria/` and `tests/` returns no match, so it stays at Textual's default `'full'`.

Measured live at the reviewed revision with `reduced_motion=True`: `app.animation_level` is `'full'`, and an `end` keypress in the transcript produces the trajectory `5.0, 9.5, 13.5, 17.1, 20.2, 23.0, 25.4, 27.4, 29.2, 30.6, 31.8, 32.8, 33.5, 34.1` sampled at 50 ms — byte-identical to the same run with the setting off. Forcing `app.animation_level = 'none'` produces `35.0, 35.0, 35.0, 35.0, 35.0, 35.0`.

The release's own visual specification makes this a pass criterion: acceptance item 26 states "spinners/pulses/easing are absent".

**What to change.**

In `talaria/ui/app.py`, immediately after the `MotionPolicy` construction at line 1208, add `self.animation_level = "none" if reduced_motion else self.animation_level`. Textual's animator gates every scroll animation on that value, so one assignment makes all key-driven scrolling instantaneous without touching `MotionPolicy` or any widget.

**Verifiably resolved when.**

- A new test in `tests/ui/test_motion.py` samples `transcript.scroll_y` at intervals after `transcript.scroll_end()` under `reduced_motion=True` and asserts the first sample already equals `max_scroll_y`. It must fail on the current code.
- A counterexample in the same test: with `reduced_motion=False` the trajectory has more than one distinct value, so the test cannot pass by always asserting instant scroll.
- A test asserts `app.animation_level == 'none'` under reduced motion and is unchanged otherwise.
- `docs/terminal-ui.md` states that reduced motion covers key-driven scrolling, not only Talaria-initiated scrolling.
- `uv run pytest tests/ui/test_motion.py tests/ui/test_transcript_bounds.py -q` is green, then the full project check is green.

---

## `fix-33e4c93833a2` — P2 (F-20)

Route `manual` -> owner `review-fixer` · session `talaria-w1`

**Exact paths.**

- `tests/domain/test_boundary.py`
- `tests/test_framework_boundaries.py`

**What is wrong.**

**Assigned to `talaria-w1` by the coordinator.** This is a new guard file rather than any child's code. It is routed here because this session has the broadest test surface on the run — it made the replay and transport harnesses theme-aware for issue #109 and built issue #104's contrast measurement — so it is the session most likely to get the sweep's coverage right. The architecture rule it protects is load-bearing, and a sweep that silently misses four packages is worse than no sweep.

`tests/domain/test_boundary.py` is excellent within its scope — a filesystem walk following symlinks, an allow-list rather than a deny-list, a subprocess for attribution — but its allow-list at `:75` is `("talaria.domain",)` and its sweep walks only the domain package. Probed on a scratch copy: injecting `import textual` into `talaria/themes/__init__.py`, `talaria/status/contract.py` and `talaria/transport/source.py` left the test at **2 passed**; the same injection into `talaria/domain/changes.py` turned it red.

`talaria/themes/` is a package this release created specifically so `talaria/config.py` could validate imported theme names without importing the user-interface layer, and `talaria/config.py` now depends on it. Adding `from textual.theme import Theme` to `talaria/themes/__init__.py` is the single most tempting future edit in that package, and it would drag Textual into every command-line verb with the whole suite green.

All four packages are framework-free today; independent import sweeps confirm it. This request is about keeping them that way.

**What to change.**

Add a sibling parameterised check running the same subprocess sweep for `talaria.themes` with allow-list `('talaria.themes',)`, for `talaria.status` with `('talaria.status', 'talaria.domain', 'talaria.recorder')`, and for `talaria.transport` with the same plus its own tree, reusing the sweep payload already in `tests/domain/test_boundary.py`. Place it at `tests/test_framework_boundaries.py` so it is not scoped to one package's test run. Leave the domain test where it is.

**Verifiably resolved when.**

- Injecting `import textual` into `talaria/themes/__init__.py` turns the suite red — verify locally, then revert.
- The same holds for `talaria/status/contract.py` and `talaria/transport/source.py`.
- The existing domain boundary test still passes unchanged.
- The full project check is green.

