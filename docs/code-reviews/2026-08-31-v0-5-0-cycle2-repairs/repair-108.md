# Repair brief — issue #108: read-only diff viewer (cycle 2)

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
| `fix-c7c45983a917` | 3 | P2 | F-49 | `manual` -> `review-fixer` | — |
| `fix-f75fd3033bef` | 3 | P2 | F-52 | `safe_auto` -> `review-fixer` | — |

**Read this before starting the first request.** Both of these are cycle-1 repairs that satisfied their acceptance criterion exactly and did not fix the thing the criterion existed to protect. The first one is the review's own fault: cycle 1 asked for a test proving F6 and F7 still reach the pickers *when focus is elsewhere*, the landed test is named exactly that, it focuses the transcript, and it passes — while F6 and F7 are now dead from the focus the application actually starts in. The acceptance criteria below are written to close that gap. Read them before you write the code, not after.

---

## `fix-c7c45983a917` — Tier 3 — P2 (F-49)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/app.py`
- `docs/terminal-ui.md`
- `tests/ui/test_composer.py`

**What is wrong.**

Commit 786c7f4 removed `priority=True` from the `f6` and `f7` application bindings at `talaria/ui/app.py:1162` and `:1167`, to stop them shadowing the composer's select-all. `talaria/ui/app.py:1154` kept it for `f3`.

Textual 8.2.8's `TextArea.BINDINGS` maps `f6` to `select_line` and `f7` to `select_all` — confirmed against the installed package. The composer's text area holds focus at launch. So the collision was not removed; it was inverted. Measured: from the default focus at launch, `f6` and `f7` call `open_picker` zero times and silently select text in the composer, while `f3` still opens the palette. An operator following the documented keybinding table presses F6 in the ordinary state of the application and never reaches the models picker.

`docs/terminal-ui.md:131-132` lists "/models or F6" as equals with no focus caveat, and the code comment calls the two aliases symmetrical while they now behave differently.

**What to change.**

Either move the two aliases to function keys Textual's `TextArea` does not claim and restore `priority=True`, or accept the composer-focus limitation and make it honest — in `docs/terminal-ui.md` and in the symmetry claim in the code comment. The slash primaries remain the intended primary route either way, which is why this is P2 rather than P1.

**Verifiably resolved when.**

- A test starts the application, makes **no focus change at all**, presses the key, and asserts the picker opened. The launch focus is the case that matters; a test that focuses something first is the test that already exists and already passes.
- A second test presses the key with the composer focused and containing text, and asserts the composer's selection is unchanged — so the fix does not simply re-shadow select-all and reintroduce cycle-1 finding 33.
- `docs/terminal-ui.md` either lists the keys with no caveat and both tests above pass, or states the focus limitation in the same sentence as the alias.
- The full project check is green.

---

## `fix-f75fd3033bef` — Tier 3 — P2 (F-52)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `tests/ui/test_diff_viewer.py`

**What is wrong.**

The autouse fixture at `tests/ui/test_diff_viewer.py:52` promises in its docstring at `:53` to make any runtime write fail loudly, and every diff-viewer test runs under it. Its patch list at `:95-98` covers three capabilities: `builtins.open`, `os.open` and `subprocess.Popen`.

`pathlib.Path.open` calls `io.open`, which is neither. The testing lens inserted `Path("/tmp/talaria-probe-c.txt").write_text(...)` into `DiffCanvas.render_line`; the file was created on disk during the test run, 11 of 12 tests passed, and only the static allowlist objected. By contrast the `tempfile` and `subprocess` probes each produced 12 failures, so the runtime guard does fire for what it patches. `os.remove`, `os.unlink`, `os.rename`, `os.mkdir` and the `shutil` operations are not guarded at all.

The read-only guarantee therefore rests entirely on the static allowlist — a large hand-maintained set that a maintainer must edit on every legitimate change, which is exactly the fragility cycle-1 finding 11 was raised about.

**What to change.**

Patch `io.open` in addition to `builtins.open`, and add guarded wrappers for `os.remove`, `os.unlink`, `os.rename`, `os.replace`, `os.mkdir`, `os.makedirs`, `os.rmdir` and `shutil.rmtree` under the same predicate.

**Verifiably resolved when.**

- Re-run the lens's probe: insert `Path(...).write_text(...)` into `DiffCanvas.render_line`, confirm the suite turns red, and remove it. Report the failure count. A green suite with only the static allowlist objecting is the current state and is not a fix.
- Repeat the probe for `os.remove` and for `shutil.rmtree` and confirm each turns the suite red.
- The static allowlist is unchanged, so the two guarantees remain independent rather than one propping up the other.
- `uv run pytest tests/ui/test_diff_viewer.py -q` is green, then the full project check is green.
