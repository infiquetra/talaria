# Repair brief — issue #108: read-only diff viewer

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (3)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-9ba02a80147e` | P1 | F-11 | `manual` -> `review-fixer` | — |
| `fix-da36d4b8df99` | P3 | F-37 | `gated_auto` -> `downstream-resolver` | — |
| `F-33 (no fix request — pre-existing)` | P2 | F-33 | `gated_auto` -> `review-fixer` | talaria-w4 |

---

## `fix-9ba02a80147e` — P1 (F-11)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `tests/ui/test_diff_viewer.py`

**What is wrong.**

The diff viewer's read-only guarantee is the release's headline safety property for rendering gateway-supplied diff content, and `tests/ui/test_diff_viewer.py` is the only thing enforcing it. Its binding-set and action-method assertions are equality checks and are behaviour-sensitive. Its call and import assertions are **blocklists** and fail open: `:562-573` forbids ten call names, `:594` forbids four module names, both parsed from the viewer module's own syntax tree.

Two probes at the reviewed revision confirm the gap. Inserting a `tempfile.NamedTemporaryFile(mode='w', ...).write(...)` — a genuine filesystem write — into `DiffCanvas` left all eleven tests passing, with Ruff and Bandit clean. Adding `talaria/ui/_diff_ops.py` containing `subprocess.run(['git','add',path])` and calling it from `DiffCanvas` also left all eleven passing, because the syntax walk only reads one module.

Control arm, so this is not a claim that the property is currently violated: adding a naive `action_apply` **did** turn the assertion at `:560` red, and the module is clean today — `talaria/ui/diff_viewer.py:15-50` imports only `re`, `dataclasses`, `difflib`, `typing`, `pygments`, `rich`, `textual` and three Talaria modules.

**What to change.**

Replace the name blocklist with a capability assertion. Drive the existing pilot tests with `builtins.open`, `os.open`, `subprocess.Popen` and the dispatcher's send method patched to raise, so any write or dispatch reached at runtime fails loudly whatever it is called. Keep the syntax-tree check as a cheap second line, invert its call and import assertions from blocklists to allowlists matching what the module uses today, and extend the walk to every `talaria.ui` module the viewer imports so a one-hop helper cannot launder the call.

**Verifiably resolved when.**

- Re-run both probes locally: inserting a `tempfile` write into `DiffCanvas` must turn the suite red, and a cross-module `subprocess.run` called from `DiffCanvas` must turn it red. Revert both.
- The naive `action_apply` probe still turns it red.
- The import assertion is an equality check, so adding any new import forces a reviewer to look.
- `uv run pytest tests/ui/test_diff_viewer.py -q` is green on the unmutated tree, then the full project check is green.

---

## `fix-da36d4b8df99` — P3 (F-37)

Route `gated_auto` -> owner `downstream-resolver`

**Exact paths.**

- `talaria/ui/diff_viewer.py`
- `talaria/ui/app.py`
- `tests/ui/test_diff_viewer.py`

**What is wrong.**

`MotionPolicy` is this release's single restart-scoped authority for whether a scroll animates, and every other scrolling surface routes through it — `talaria/ui/transcript.py` at five call sites and `talaria/ui/prompts.py` at two, each doing `motion = self.motion.scroll(animate=False)` then `animate=motion.animate`. The diff viewer never receives a `MotionPolicy` and answers the question itself with a literal `animate=False` at `talaria/ui/diff_viewer.py:483` and `:490`.

There is no behavioural divergence today because the hardcoded answer matches what the policy returns, which is precisely why it will go unnoticed: relaxing `MotionPolicy` to permit easing under standard motion would change every surface except this one, silently.

Depends on issue #109's `MotionPolicy` remaining the single authority; coordinate ordering if #109's own motion repair lands first.

**What to change.**

Pass the app's `MotionPolicy` into `DiffViewer` the way `talaria/ui/app.py:1630-1636` already passes it to `TranscriptPane` and `PromptRegion`, and replace the two literals with `animate=motion.animate, duration=motion.duration`.

**Verifiably resolved when.**

- `grep -n 'animate=False' talaria/ui/diff_viewer.py` returns nothing.
- A test asserts the diff viewer's scroll respects a `MotionPolicy` constructed with `reduced=False`, so the wiring is observable rather than coincidental.
- `uv run pytest tests/ui/test_diff_viewer.py tests/ui/test_motion.py -q` is green, then the full project check is green.

---

## `F-33 (no fix request — pre-existing)` — P2 (F-33)

Route `gated_auto` -> owner `review-fixer` · session `talaria-w4`

**Exact paths.**

- `talaria/ui/app.py`
- `tests/ui/test_composer.py`

**What is wrong.**

**Assigned to `talaria-w4` by the coordinator.** These bindings live in the global registry in `talaria/ui/app.py`, a leased shared surface, and both predate this release. It is routed here because this session added the `/diffs` binding and the `ctrl+b`-adjacent wiring most recently, so it has the live picture of what the registry has to reconcile.

`talaria/ui/app.py:1162` registers `f6` and `:1167` registers `f7`, both with `priority=True`, so they fire ahead of the focused widget's own bindings. Textual 8.2.8's `TextArea` binds `f6` to `select_line` and `f7` to `select_all`, and maps `ctrl+a` to `cursor_line_start` rather than select-all — so there is no alternative. Measured live: with two lines in the focused composer, pressing `f7` and then `f6` both leave `selected_text` empty.

An operator therefore cannot select all the text in the composer by keyboard, and replacing a long drafted prompt requires repeated shift-arrow passes. This is keyboard-only breakage; a pointer user can still drag-select. Both bindings exist at the diff base, so this release did not introduce it — but the run plan's rule against stealing ordinary composer editing keys is exactly the rule it breaks.

Because `talaria/ui/app.py` is one of the three leased shared surfaces, take the lease before editing it.

**What to change.**

Drop `priority=True` from the `f6` and `f7` bindings so a focused `TextArea` keeps its own `select_line` and `select_all`, leaving the slash-command routes `/models` and `/profiles` as the primary surface the plan already names. If the aliases must fire from inside the composer, move them to unused function keys instead.

**Verifiably resolved when.**

- A test in `tests/ui/test_composer.py` asserts `f7` in the focused composer selects the full document. It must fail on the current code.
- A test asserts `f6` and `f7` still reach the model and profile pickers when focus is elsewhere.
- The full project check is green.

