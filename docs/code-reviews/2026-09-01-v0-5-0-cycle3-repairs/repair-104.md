# Repair brief — issue #104: theme token foundation (cycle 3)

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

## Requests (2 approved, 1 carried)

| Request | Severity | Findings | Route | Touches `talaria/`? | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-751dcb874bb2` | P3 | F-91 | `safe_auto` -> `review-fixer` | **YES — invalidates the receipts** | — |
| `fix-232e7fb01551` | P3 | F-88 | `manual` -> `review-fixer` | **YES — invalidates the receipts** | — |
| `F-92 (carried — not in the sixteen)` | P3 | F-92 | `safe_auto` -> `review-fixer` | yes, but not in the round | — |

**Soft ordering.** Take `fix-751dcb874bb2` before `fix-232e7fb01551`. Both edit `talaria/config.py`, at line 368 and line 451. This is conflict avoidance, not a correctness ordering.

**Both approved requests touch product code**, so this lane is on the re-drive path. Land both together in one commit if you can.

---

## `fix-751dcb874bb2` — P3 (F-91)

Route `safe_auto` -> owner `review-fixer`. **Touches `talaria/config.py`.**

**Exact paths.** `talaria/config.py`, `tests/test_config_write.py`

**What is wrong.**

This is residue from the cycle-2 symlink repair, and that repair is otherwise sound — the security lens ran the original attack end to end this cycle and confirmed the content escape is closed in both directions, including the counter-probe that `save_theme` still follows a symlinked `config.toml`. Do not disturb either.

What survives is narrower. `talaria/config.py:365` resolves `target` only when `follow_symlinks` is true, but `:368` then calls `target.stat()`, which follows symlinks unconditionally. So when `follow_symlinks` is false and the path is a symlink, the function correctly refuses to write *through* the link — and still copies the link target's permission bits onto the replacement file.

Measured with `follow_symlinks` at its default and the process umask at 0o022: a symlink pointing at a mode 0666 file was replaced by a regular file at **0666** rather than the expected 0644; a symlink to a 0600 file produced a replacement at 0600. In both probes the target file's content was unchanged, so this is residue rather than a reopening.

The consequence is narrow, because planting the link already requires write access to the operator's themes directory. But the outcome is that an attacker with that access can leave behind a **world-writable stored theme** that any other local account can then rewrite, which is a wider result than simply writing the theme themselves. It is also a plain internal inconsistency: a function whose documented contract is that a symlink at the path is itself replaced should not consult the target for any property.

**What to change.**

Read the existing mode with `os.lstat(target)` rather than `target.stat()`, and use `os.path.lexists` rather than `target.exists()` for the guard, so that when `follow_symlinks` is false no property of the link target influences the replacement. When `follow_symlinks` is true, `target` has already been resolved, so `lstat` and `stat` agree and the `save_theme` path is unaffected.

**Verifiably resolved when.**

- A test replaces a symlink pointing at a **0666** file and asserts the replacement lands at the umask default, not 0666. Assert the mode, not merely that the write succeeded.
- A test asserts `save_theme` through a symlinked `config.toml` still writes the link's target and preserves that target's mode — the cycle-1 behaviour, pinned rather than assumed.
- A test asserts the link is replaced by a regular file and the outside file's bytes are unchanged, so the cycle-2 P1 stays closed.
- The full project check is green.

---

## `fix-232e7fb01551` — P3 (F-88)

Route `manual` -> owner `review-fixer`. **Touches `talaria/config.py`.**

**Exact paths.** `talaria/config.py`, `tests/test_config.py`

**What is wrong.**

The cycle-2 repair that made the inline theme table work changed the rewrite helper to replace only the quoted *value* rather than the whole line. Against a TOML multi-line basic string the value pattern at `talaria/config.py:117-121` matches the empty pair of quotes at the front of the triple quote and replaces only that, producing bytes that are not valid TOML. `save_theme` then parses the bytes it just produced and raises a message naming **the operator's file** as not valid TOML.

Measured: `save_theme('solar-flare', 'user')` against a file whose only content is `theme.name = """midnight"""` raises `ConfigError '<path> is not valid TOML: Expected newline or end of document after a statement (at line 1, column 27)'`. That same input file parses cleanly under `tomllib`. The file was valid; Talaria's rewrite was not.

Nothing is written — `save_theme` reparses and compares before calling `atomic_replace_bytes`, so this **fails safe**, and the correctness lens confirmed the safety net is real by planting a nested inline table that makes the pattern rewrite the wrong key and watching the function refuse with `the edit changed more than theme.name`. The defect is the diagnostic: the operator is sent looking for a syntax error in a file that does not have one, which is the opposite of what a safe failure should tell them.

For scale: the same call succeeded against thirteen other shapes — dotted, dotted with a trailing comment, single-quoted, quoted key, inline table spaced and tight, inline with a trailing comment, inline with a second key, Windows line endings, a `[theme]` header, an empty file, and a file with no theme table — preserving comments and line endings throughout.

**What to change.**

Either widen the value pattern to cover TOML multi-line basic and literal strings, or catch the parse failure of the **rewritten** bytes separately and say that Talaria could not rewrite `theme.name` in this file's form — instead of passing the parse error through as a claim about the file the operator wrote.

**Verifiably resolved when.**

- A test saves a theme against a `theme.name = """..."""` file and asserts either the rewrite succeeds with only the name changed, or the error names Talaria's own limitation rather than the file's validity. Assert on the message text, since the message is the defect.
- The same for a multi-line literal string.
- The thirteen working shapes still work; parametrise them so the set is visible rather than incidental.
- The full project check is green.

---

## `F-92 (carried — not in the sixteen)` — P3 (F-92)

Route `safe_auto` -> owner `review-fixer`. **Not part of the approved round** — pre-existing.

**Exact paths.** `talaria/ui/blocks.py`, `talaria/transport/refresh.py`, `talaria/transport/admin.py`, `.github/workflows/validate.yml`

**What is wrong.**

All twelve `# nosec` suppressions in `talaria/` are **correct today** — the security lens parsed each one exactly as bandit does and confirmed every one resolves to precisely the single test identifier it names. The review controller's initial reading that they might be inert was wrong, and `metrics nosec: 0` is the *desirable* value: bandit counts blanket suppressions there and targeted ones under `skipped_tests`.

The residual risk is the comment style. The form `# nosec B603 - prose explanation` makes bandit parse each prose word as a further identifier, emitting a warning per word — **twenty per run** in this tree, from words like `here`, `bandit`, `s`, `f` and `string`. A *mistyped* identifier produces a warning of the same shape and is therefore invisible in that noise. And bandit treats an unresolvable identifier list as an empty set, which means suppress everything on that line.

Proved end to end: a file with `return eval(s)  # nosec B603 - correctly spelled` still had B307 reported; the identical file with `# nosec B6O3` (letter O for zero) had B307 **suppressed**, with bandit's metrics recording `nosec=1`. So the failure mode is a silent downgrade from a targeted suppression to a blanket one, on a gate whose job includes catching hardcoded credentials.

**What to change, if the coordinator scopes it in.**

Move the explanation to its own comment line above the code and leave only the bare identifier on the suppressed line — five of the twelve sites already use that shape. Then, once the noise is gone, add a check to the workflow that fails when bandit emits `is not a test name or id`. That converts a mistyped suppression from silent to loud without enumerating identifiers anywhere.

**Verifiably resolved when.**

- `uv run bandit -r talaria -q` emits zero `is not a test name or id` warnings.
- A mistyped identifier fails the workflow, proved by planting one and watching the check fail.
