# Repair brief — issue #106: true-bottom status bar (cycle 3)

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

## Requests (2 approved)

| Request | Severity | Findings | Route | Touches `talaria/`? | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-c5fa2bba78b8` | P3 | F-81, F-84 | `manual` -> `review-fixer` | **YES — invalidates the receipts** | — |
| `fix-019217aad2ef` | P3 | F-106 | `safe_auto` -> `review-fixer` | no — **preserves the receipts** | — |

**Hard ordering, and it reaches outside this brief.** `fix-019217aad2ef` rewrites the width-band table in `docs/terminal-ui.md`. Issue #108's `fix-5fb6e79d5a8f` rewrites the keybinding table in the same file and may change which keys it names. Take #108's first, or resolve one conflict deliberately.

**Two shared surfaces.**

- `talaria/ui/app.py` — you edit the model-identity helper at `:1573`; issue #107 edits the no-text region map at `:6374`. Different regions, same file, same round. Coordinate.
- `tests/test_framework_boundaries.py` — issue #109 owns it, and F-81's repair must add the new framework-free module to that sweep. #109 has no request touching the file this round, so your edit is unopposed. Say so in the commit message.

---

## `fix-c5fa2bba78b8` — P3 (F-81, F-84)

Route `manual` -> owner `review-fixer`. **Touches `talaria/status/contract.py`, `talaria/ui/literal.py` and `talaria/ui/app.py`.**

**Exact paths.** `talaria/status/contract.py`, `talaria/ui/literal.py`, `talaria/ui/app.py`, `tests/test_framework_boundaries.py`, `tests/ui/test_status_bar.py`, `tests/ui/test_inspector.py`

### F-81 — a second, weaker control-character rule

**What is wrong.**

Talaria has one function that turns an untrusted string into something a terminal will draw rather than obey: `defang`, at `talaria/ui/literal.py:175`, backed by a roughly 430-codepoint translation table. Its docstring states the design rule in the imperative — it is "deliberately one function rather than a strict version for commands and a lenient one for prose, because two rules means one of them is eventually applied to the wrong string."

This release adds a second rule. `talaria/status/contract.py:303` defines `_defang_status_segment` over a ten-entry table plus a catch-all for bytes below 0x20. Measured on eight hazard inputs, the two agree on two and **disagree on six**: a right-to-left override, a zero-width joiner, a byte-order mark, a soft hyphen, a Unicode Tag letter and a left-to-right isolate all pass through the new rule untouched and are marked by the canonical one.

Nothing is broken today, because the caller wraps the result in Python's `repr()`, and `repr()` escapes all eight. That is the problem in miniature, and the lens proved it: replacing `_defang_status_segment` with the identity produces a notice showing `‮` either way. **The new function contributes nothing to safety that `repr()` was not already contributing**, while establishing a second, weaker authority under a name that promises the first one's guarantee. The next caller who reads the name `defang` and uses it without `repr()` gets six hazard classes straight through.

The reason the copy exists is good: `talaria/status/contract.py` must not import `talaria.ui`, and `tests/test_framework_boundaries.py` enforces that. **But the constraint forces a module move, not a second rule.**

**What to change.**

Move the translation table and `defang` out of `talaria/ui/literal.py` into a framework-free home — a new `talaria/text.py`, or an existing module under `talaria/domain/`. `talaria/ui/literal.py` keeps `literal_text` and re-exports `defang` so its nine current importers are unchanged; `talaria/status/contract.py` imports the same function, and `_NOTICE_CONTROL_PICTURES` and `_defang_status_segment` are deleted. Add the new module to the framework-free sweep so the move is held.

If moving is judged too large, the minimum is to rename `_defang_status_segment` to something that does not promise the `defang` guarantee, and to say in its docstring that `repr()` is the containment.

### F-84 — the model-identity helper's docstring contradicts its body

**What is wrong.**

The cycle-2 repair correctly replaced a copied roster lookup with one helper, `_focused_agent_identity` at `talaria/ui/app.py:1573`, that the status bar and the inspector both call. That extraction is sound. What it wrote down is not.

The helper returns a provider, a model, and a boolean the docstring explains as: the model was observed only in the session roster, "that row does not carry a provider, so any provider paired with it is a catalogue inference rather than part of the same observation."

The body does not do that. On the roster path it pairs a provider only when the catalogue's current model equals the roster row's model, and pairs the empty string otherwise. So **the flag is true precisely when the provider was corroborated** — the one case where it is *not* an inference. When it genuinely would be an inference the provider is already empty and the join drops it.

The two callers then disagree. Measured with a roster row for `claude-opus-4` and a catalogue whose current model matches under provider `anthropic`: the helper returns `('Anthropic', 'claude-opus-4', True)`, the status bar renders `Anthropic/claude-opus-4`, and the inspector renders `claude-opus-4` — at the same instant for the same session. The divergence is not new; it was the behaviour before the extraction. What is new is that it is now written into a shared abstraction with a stated rationale that does not describe the code.

**What to change.**

Decide which surface is right and make them agree, or keep the divergence and rewrite the docstring truthfully. If the divergence is intended, rename the flag to say what it actually distinguishes — `provider_from_catalogue_match` — and state in the docstring that the inspector deliberately shows the bare model on the roster path, with the reason.

**Verifiably resolved when (both findings).**

- `python -c "import talaria.status.contract, sys; assert 'textual' not in sys.modules"` still succeeds, and the same for whichever module now holds `defang`.
- A test runs **the same eight hazard inputs through the single remaining function** and asserts each is marked. Parametrise over the character classes `talaria/ui/literal.py` already enumerates — not over one ESC byte, which is the current test and which cannot distinguish the function from the identity.
- `grep -c 'defang' talaria/` shows one definition.
- A test asserts both surfaces' rendered strings for one roster-only state, so the intended divergence is pinned rather than incidental.
- Deleting the body of the model-identity helper turns a named test red. Run that probe and say which test caught it.
- The full project check is green.

**Acceptance consequence.** Three files under `talaria/` change. Landing this forces a re-drive; five other requests are on the same path.

---

## `fix-019217aad2ef` — P3 (F-106)

Route `safe_auto` -> owner `review-fixer`. **Depends on issue #108's `fix-5fb6e79d5a8f`.**

**Exact paths.** `docs/terminal-ui.md`

**What is wrong.**

The table at `docs/terminal-ui.md:48` is headed `Width | Default result` and gives `32-47` and `20-31` as two separate rows with different described results, which tells a reader something changes at 32 columns. Nothing does.

Measured by calling `_breakpoint` directly: widths 47, 33, 32, 31 and 20 all return form `compact` with the identical dropped set `['agent_model','context','cwd','git_branch','version']`. Width 48 returns a different set and width 19 returns form `minimum`, so **48 and 20 are the real boundaries**. `talaria/ui/status_bar.py:400-419` contains seven width branches — 144, 120, 96, 80, 64, 48 and 20. There is no branch at 32.

The number 32 is a breakpoint in the **inspector** — `talaria/ui/inspector.py:29` defines `NARROW_OVERLAY_INSET_BREAKPOINT = 32` — so a reader carrying it across from this table will predict status-bar behaviour that does not occur. The table lists nine rows against seven code branches.

This is residual of the cycle-2 F-64 repair, not a return of it: the original defect, a table dropping a segment the code renders, is fixed. Two lenses agree the code is right and one branch covering 20 through 47 correctly encodes a specification whose two rows prescribe the same segment set. The defect is that the *table* implies a boundary.

**What to change.**

Merge the two rows into a single `20-47 | Also drop agent_model` row, and leave the existing paragraph below the table to explain that within that band `task_progress` and `connection` start compact and overflow may still shorten or drop `task_progress`. Every documented boundary then equals a real branch in `_breakpoint`.

**Verifiably resolved when.**

- A test **builds the expected table rows from `_breakpoint` by walking widths** and asserts they match the document. Do not assert two pasted literal rows — that is the current test at `tests/ui/test_status_bar.py:284`, it pins exactly the two rows this cycle edited, and changing any of the other six boundaries leaves the documentation stale with nothing failing.
- Every boundary named in `docs/terminal-ui.md` equals a branch in `_breakpoint`.
- Your edit sits on top of #108's keybinding-table change, not under it.
- The full project check is green.
