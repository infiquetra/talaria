# Repair brief — issue #104: theme token foundation

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (4)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-f65bdcd773f6 (part A)` | P2, P3 | F-18, F-21, F-36 | `gated_auto` -> `review-fixer` | — |
| `fix-e4824f8bcf50` | P3 | F-39 | `safe_auto` -> `review-fixer` | — |
| `fix-9f9329adccac` | P3 | F-38 | `safe_auto` -> `review-fixer` | — |
| `F-17 (no fix request — pre-existing)` | P2 | F-17 | `gated_auto` -> `review-fixer` | — |

---

## `fix-f65bdcd773f6 (part A)` — P2, P3 (F-18, F-21, F-36)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/config.py`
- `talaria/themes/__init__.py`
- `talaria/ui/theme.py`
- `tests/test_config.py`
- `tests/ui/test_theme.py`

**What is wrong.**

Three separate defects in the theme foundation share the same files.

**F-18 (P2).** A `config.toml` that writes `theme.name = "x"` as a dotted key instead of a `[theme]` table is valid TOML that `load_config` reads correctly, but `_rewrite_theme_name` at `talaria/config.py:390-397` finds no literal `[theme]` header and appends one, producing a document that declares the table twice. The reparse at `talaria/config.py:439` then raises `ConfigError: ... is not valid TOML: Cannot declare ('theme',) twice`, naming the operator's file as malformed when the malformed thing is the edit Talaria just built. Nothing is corrupted — the guard runs before the write — but `/theme save` fails permanently on that file with a diagnostic that sends the operator to look at a file that parses fine.

**F-21 (P2).** The theme fallback policy and its exact operator-facing sentence exist twice, at `talaria/config.py:277-287` and `talaria/ui/theme.py:153-166`, character for character, on two different code paths, with no shared constant and no test asserting they agree.

**F-36 (P3).** Two public functions named `load_user_theme_specs` exist, at `talaria/themes/storage.py:82` and `talaria/ui/theme.py:241`, differing only in whether `config_dir` defaults. `talaria/ui/theme.py:21` imports the first under the alias `load_stored_theme_specs`, so a reader sees a call to one name inside a function with the other.

**What to change.**

F-18: before appending a `[theme]` header, check the already-parsed before-document for an existing theme table; when one exists, rewrite the dotted assignment in place with a pattern matching `^[ \t]*theme[ \t]*\.[ \t]*(?:name|"name"|'name')[ \t]*=`, preserving inline comments the way `_THEME_NAME_RE` already does. If that is judged too much surface, at minimum raise a distinct `ConfigError` naming the real cause.

F-21: move both message templates into `talaria/themes/__init__.py` (already framework-free and already imported by both call sites) as constants with one formatting helper, and call it from both places.

F-36: rename the wrapper at `talaria/ui/theme.py:241`, or fold its `global_config_dir()` default into `theme_registry_for_config` and delete it, then drop the `load_stored_theme_specs` alias.

**Verifiably resolved when.**

- A new test writes a `config.toml` containing `theme.name = "midnight-ink"` as a dotted key, calls `save_theme('aurora-slate', 'user')`, and asserts the save succeeds and the file still parses with exactly `theme.name` changed. That test must fail on the current code.
- A new test asserts the notice `load_config` produces for an unknown theme slug is byte-identical to the notice `ThemeRegistry.resolve` produces for the same slug.
- `grep -c 'must be a string; using Refined Default' talaria/` returns 1, not 2.
- `grep -rn 'def load_user_theme_specs' talaria/` returns exactly one definition.
- `uv run pytest tests/test_config.py tests/test_config_write.py tests/ui/test_theme.py -q` is green, then `uv run ruff check . && uv run mypy && uv run pytest` is green.

---

## `fix-e4824f8bcf50` — P3 (F-39)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/config.py`
- `tests/test_config.py`

**What is wrong.**

When an operator writes a top-level scalar `theme = "refined-default"` instead of a `[theme]` table, `talaria/config.py:274-275` reads the scalar itself as the requested name, finds it in the available slugs, and takes neither corrective branch. The merged tree keeps `theme` as a bare string, so `cfg.get("theme", "name")` returns `None`. `ThemeRegistry.resolve` later emits "theme.name must be a string" — about a value that is a string and is a real theme, which is the least actionable form the message could take. The neighbouring case `theme = "nope"` does rewrite to a table, so the two scalar cases behave inconsistently with each other.

**What to change.**

In `_normalize_config`, require `isinstance(theme, Mapping)` before reading `name`, rewrite `merged['theme']` to a table in every branch, and emit a notice naming the shape problem — for example `theme must be a table with a name key; using Refined Default`.

**Verifiably resolved when.**

- A new test writes `theme = "refined-default"` at the top level and asserts `cfg.get('theme', 'name')` is a string and that the notices tuple names the table shape. It must fail on the current code.
- `uv run pytest tests/test_config.py -q` is green, then the full project check is green.

---

## `fix-9f9329adccac` — P3 (F-38)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/ui/palette.py`

**What is wrong.**

`TALARIA_LOCAL_COMMANDS` now holds thirteen rows, but `talaria/ui/palette.py:12` still says "Talaria's own four" and `talaria/ui/palette.py:106` still says "The seven Talaria-local entries". The rendered listing is correct because the builder iterates the tuple, so nothing breaks — but two stale counts in the file that renders the listing are the strongest available signal that a consumer was missed when the command set grew, and a reviewer who trusts them will hunt for a fixed-size allowlist that does not exist.

**What to change.**

Replace both fixed counts with a reference to the data: at line 12 say `local` marks each of Talaria's own controls in `TALARIA_LOCAL_COMMANDS`, and at line 106 drop the number entirely.

**Verifiably resolved when.**

- `grep -nE "own four|seven Talaria-local" talaria/ui/palette.py` returns nothing.
- No digit naming a command count remains in the module docstring or the builder docstring.
- `uv run pytest tests/ui/test_slash_palette.py -q` is green, then the full project check is green.

---

## `F-17 (no fix request — pre-existing)` — P2 (F-17)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/config.py`
- `tests/test_config.py`

**What is wrong.**

`atomic_replace_bytes` at `talaria/config.py:337-353` calls `os.replace(temp_path, path)` without resolving `path`, while the read side at `talaria/config.py:430` uses `path.read_bytes()`, which follows symlinks. Read and write therefore disagree about which file they mean. An operator whose `~/.talaria/config.toml` is symlinked into a dotfiles repository — the ordinary chezmoi, stow and yadm arrangement — has the link replaced by a regular file, their real file keeps the old theme, and Talaria reports a successful save. `docs/configuration.md:100-103` promises the file is replaced atomically.

This is marked **pre-existing**: the helper predates this release. It is routed here because issue #104 is the first caller to point it at an operator-authored file, so it is now reachable.

**What to change.**

Resolve the target before writing: compute `target = path.resolve() if path.is_symlink() else path` and use `target` for both the temporary-file directory and the `os.replace`, so the temporary file lands beside the real file and the rename stays atomic on one filesystem. Preserve the existing mode from `target.stat().st_mode` rather than leaving `mkstemp`'s 0600.

**Verifiably resolved when.**

- A new test creates a real config file, symlinks a second path to it, calls `save_theme` through the link, and asserts the link is still a symlink afterwards and its target carries the new theme name. It must fail on the current code.
- A second assertion checks the target's file mode is unchanged by the save.
- `uv run pytest tests/test_config.py tests/test_config_write.py -q` is green, then the full project check is green.

