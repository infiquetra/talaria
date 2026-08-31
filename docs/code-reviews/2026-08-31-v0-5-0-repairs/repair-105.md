# Repair brief — issue #105: Visual Studio Code theme import

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (7)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-f7593197c3d1` | P1 | F-3 | `safe_auto` -> `review-fixer` | — |
| `fix-f65bdcd773f6 (part B)` | P1 | F-2 | `gated_auto` -> `review-fixer` | — |
| `fix-8dea4e57a7f7` | P2 | F-14 | `gated_auto` -> `review-fixer` | — |
| `fix-bd91ff76f38d` | P2 | F-35 | `manual` -> `review-fixer` | — |
| `fix-f0185fd1bb3a` | P2 | F-27, F-29 | `manual` -> `downstream-resolver` | — |
| `fix-88f7ff757945` | P2 | F-25 | `manual` -> `human` | — |
| `F-19 (no fix request — pre-existing)` | P2 | F-19 | `gated_auto` -> `review-fixer` | talaria-w2 |

**Ordering.**

- `fix-8dea4e57a7f7` **must land before** `fix-bd91ff76f38d`. The drift guard asserts the document equals the code, so the fourteen-to-eighteen correction must land first or the guard locks in the wrong table.
- `fix-f65bdcd773f6 (part B)` is better landed before `F-19 (no fix request — pre-existing)`. The stored-theme guard removes the StoredThemeError escape at its source; the entry-point handler is the backstop for ConfigError and should be written against the narrowed set.

---

## `fix-f7593197c3d1` — P1 (F-3)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/cli.py`
- `talaria/ui/theme_import.py`
- `tests/test_cli.py`

**What is wrong.**

The importer builds its operator-facing warning lines by interpolating raw JSON keys from the source theme file — `talaria/ui/theme_import.py:354` (`path = f"root.{key}"`), `:412`, `:520` and `:527` — and `talaria/cli.py:247` prints them with a bare `print(line, file=stream)` with no defang. An operator importing a theme downloaded from the internet hands Talaria a file whose keys are attacker-chosen, so an escape sequence embedded in a key is obeyed by the terminal rather than shown: the screen can be cleared, the window retitled, or the cursor re-homed to overwrite prior output.

Reproduced at the reviewed revision with a theme whose colour key contains `ESC ] 0 ; PWNED BEL ESC [ 2 J`: stderr came back as the literal bytes `warning: colors.\x1b]0;PWNED\x07\x1b[2Jjunk is unsupported`, two raw `ESC` bytes, exit status 0.

`talaria/ui/literal.py:9-12` names those exact two sequences as the threat its `defang` helper exists to stop, so this is the project's own control being bypassed on a path it never covered. `talaria/ui/theme_import.py:562` already uses `{selector!r}` and is safe, so the escaping is inconsistent rather than absent by design.

**What to change.**

Defang at the render boundary, which is the one place that cannot be forgotten: import `defang` from `talaria.ui.literal` and wrap both `talaria/cli.py:247` and the error print at `talaria/cli.py:242`. Belt and braces at source: change the four interpolations at `talaria/ui/theme_import.py:354`, `:412`, `:520` and `:527` to the `{key!r}` form line 562 already uses.

**Verifiably resolved when.**

- A new test in `tests/test_cli.py` imports a theme whose JSON key contains `chr(27)` and asserts `chr(27)` is absent from both captured streams. It must fail on the current code.
- A byte-level check, not a string check: assert `b'\x1b' not in captured_stderr.encode()`.
- `uv run pytest tests/test_cli.py tests/ui/test_theme_import.py -q` is green, then the full project check is green.

---

## `fix-f65bdcd773f6 (part B)` — P1 (F-2)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/themes/storage.py`
- `talaria/config.py`
- `tests/test_config.py`
- `tests/ui/test_theme_import.py`

**What is wrong.**

`load_config` walks `<TALARIA_CONFIG_DIR>/themes/*.json` and calls `load_user_theme_specs` at `talaria/config.py:493-495` with no exception guard. `talaria/themes/storage.py:48` enforces exact set equality on the stored-theme field set, so any file there that is not exactly canonical raises `StoredThemeError`, and nothing in `talaria/cli.py` catches it. The process dies with a Python traceback before the terminal interface opens — the operator loses the application, not one theme.

Five distinct file states reproduce it: an extra field, unrelated JSON, truncated JSON, an empty file, and a theme missing tokens. Dropping a raw Visual Studio Code theme file into the theme directory is the obvious thing an operator will try, and its field set is not `{dark, name, slug, tokens}`, so it bricks every launch path.

Separately, `load_config`'s own docstring at `talaria/config.py:468` promises `ConfigError`, so any caller writing `except ConfigError` is silently wrong.

Note for the coordinator: the consensus engine merged this with three #104 findings because they share `talaria/config.py`. It is split out here because the run's commit rule forbids a cross-child commit.

**What to change.**

Make `load_user_theme_specs` return a `(specs, notices)` pair that skips each unreadable file and names it in a notice, instead of raising on the first one, and have `talaria/config.py:493-499` append those notices to the tuple `_normalize_config` already returns. A bad stored theme then degrades exactly like an out-of-range `status.cwd_max_columns` does. Keep the strict per-file validation available under a separate strict entry point so `tests/ui/test_theme_import.py:356` still has something to assert against.

**Verifiably resolved when.**

- A new test writes `themes/broken.json` containing `{"bogus": 1}`, calls `load_config()`, and asserts it returns normally with a notice naming that path. It must raise on the current code.
- Cover all five states: extra field, unrelated JSON, truncated JSON, empty file, missing tokens.
- A counterexample in the same test: a valid stored theme beside the broken one is still discovered, so the fix cannot pass by returning an empty library.
- `load_config`'s docstring at `talaria/config.py:468` is either honoured or corrected — no exception type escapes it that the docstring does not name.
- `uv run pytest tests/test_config.py tests/ui/test_theme_import.py -q` is green, then the full project check is green.

---

## `fix-8dea4e57a7f7` — P2 (F-14)

Route `gated_auto` -> owner `review-fixer`

**Exact paths.**

- `docs/formats/vscode-theme-import.md`
- `docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`

**What is wrong.**

`docs/formats/vscode-theme-import.md:96` says "These fourteen Talaria extension tokens have no entry in either supported mapping", and closes with "Any other token falls back only when its listed source is absent or invalid." Eighteen tokens have no mapping. The four extras are `talaria.status.success`, `talaria.status.warning`, `talaria.status.error` and `talaria.status.attention`, because `talaria/ui/theme_import.py:63-65` maps only `statusBar.background`, `statusBar.foreground` and `statusBar.border`.

The repository's own test already knows this: `tests/ui/test_theme_import.py:73-87` asserts `len(ALWAYS_FALLBACK_TOKENS) == 14` and then asserts exactly those four further always-fallback tokens. A live import supplying every mapped candidate key reported "40 source tokens, 18 fallbacks, 0 warnings".

The consequence for an operator: an imported dark theme silently takes Refined Default's light-palette connection, warning, error and attention colours in the bottom bar, with no way to learn why from the document that calls itself the public allowlist.

`docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md:668` carries the identical sentence, and its example report at `:693` reads "37 source tokens, 17 fallbacks", which sums to 54 against a 58-token vocabulary.

**What to change.**

Rename the section to eighteen tokens and add four rows for the status state tokens with the reason that Visual Studio Code has no bounded per-state status-bar colour role. Make the identical edit in the visual specification so the declared verbatim relationship survives, and correct the example report to a pair that sums to 58.

**Verifiably resolved when.**

- The fallback table in `docs/formats/vscode-theme-import.md` has eighteen rows and names all four `talaria.status.*` state tokens.
- The same table in the visual specification is byte-identical to it.
- The specification's example report counts sum to 58.
- The full project check is green.

---

## `fix-bd91ff76f38d` — P2 (F-35)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `tests/ui/test_theme_import.py`
- `docs/formats/vscode-theme-import.md`

**What is wrong.**

`docs/formats/vscode-theme-import.md:4` declares itself "the public allowlist" and `:7-8` declares its mapping and resolution rules "copied verbatim from the v0.5.0 visual specification". Nothing compares either table to `talaria/ui/theme_import.py` or to the specification. The single TOML example in the documentation got a drift guard (`tests/test_config.py:527-553`) and stayed correct; the mapping tables got none, and the fallback table is already wrong by four rows. Every future change to `WORKBENCH_MAPPINGS` or `ALWAYS_FALLBACK_TOKENS` will silently desynchronise two documents a reader is told to trust as normative.

**What to change.**

Add a test beside `tests/ui/test_theme_import.py` that parses the workbench, syntax-scope and fallback tables out of `docs/formats/vscode-theme-import.md` and asserts them equal to `WORKBENCH_MAPPINGS`, `SYNTAX_MAPPINGS` and the set of tokens with no mapping, and separately asserts the three tables are byte-identical to their counterparts in the visual specification.

**Verifiably resolved when.**

- The new test fails if a row is added to `WORKBENCH_MAPPINGS` without updating the document — demonstrate this by adding a throwaway row locally, observing the failure, and reverting it.
- The new test fails if the two documents' tables diverge by one character.
- It passes on the corrected documentation from fix-8dea4e57a7f7, so land that first.
- The full project check is green.

---

## `fix-f0185fd1bb3a` — P2 (F-27, F-29)

Route `manual` -> owner `downstream-resolver`

**Exact paths.**

- `talaria/cli.py`
- `talaria/ui/theme_import.py`
- `tests/test_cli.py`

**What is wrong.**

**F-27.** The import report is free-form English routed between stdout and stderr by testing whether a line literally begins with `warning: ` (`talaria/cli.py:245-248`). There is no machine-readable option, no versioned report shape, and no documented line grammar, so a script wanting the fallback count must parse prose that no test pins. The two-stream split also destroys ordering: a consumer capturing both streams sees interleaving that depends on buffering, so a warning cannot be reliably associated with the fallback line it followed.

**F-29.** `talaria/cli.py:241-243` returns exit 2 for every import failure — missing file, empty file, malformed JSON, array root, string root, reserved slug, invalid slug, unwritable target — and argparse's own usage errors also return 2. Eight cases calling for different automated responses are indistinguishable without matching English error text that no test pins.

**What to change.**

F-27: add a `--json` flag to the `theme import` subparser writing one object to stdout carrying `schema_version`, slug, target path, source-token count, fallback count, warning count, and arrays of fallback and warning records; keep every line of a `--json` run on stdout so ordering survives; leave the prose as the default. Give the report object a severity field rather than deriving severity from a rendered string prefix.

F-29: give `ThemeImportError` a `kind` attribute from a small closed set (unreadable, empty, malformed, wrong-root, reserved-slug, invalid-slug, unwritable) and map it to distinct exit codes in `run_theme_import_command` — for example 3 for a source-document problem, 4 for a naming problem, 5 for a write problem — leaving 2 for argparse usage errors alone.

**Verifiably resolved when.**

- A test asserts `--json` output parses as one JSON object carrying every named field, on stdout only, with an empty stderr.
- A test asserts each of the seven failure kinds returns its own exit code, and that an argparse usage error still returns 2.
- The exit-code table is documented in `docs/formats/vscode-theme-import.md`.
- The default prose output is unchanged — an existing test pins it.
- The full project check is green.

---

## `fix-88f7ff757945` — P2 (F-25)

Route `manual` -> owner `human`

**Exact paths.**

- `talaria/themes/storage.py`
- `docs/formats/stored-theme.schema.json`

**What is wrong.**

The stored theme document is a closed four-field set enforced by exact set equality at `talaria/themes/storage.py:48`, with no `schema_version`, no format version, and no declared JSON Schema anywhere in the repository. No later Talaria can add a field without every earlier Talaria rejecting the file, and no earlier Talaria can distinguish "this document is from a newer format" from "this document is corrupt". The sibling on-disk formats this same release ships do carry version boundaries — `docs/acceptance/v0.5.0/artifact-manifest.schema.json:10` requires a `schema_version` constant — so the theme format is the one new format with no migration path.

Combined with fix-f65bdcd773f6 part B, this converts a future additive change into a launch failure for every installed earlier version, which is why it is routed as a decision rather than an edit.

**What to change.**

Add a required `schema_version` string field with the constant value `talaria-theme-v1` to the payload `serialize_user_theme` writes, accept it in the field-set check, and treat an unrecognised value as skip-with-notice rather than a hard error. Commit a `docs/formats/stored-theme.schema.json` alongside the prose. Existing files on operator machines carry no `schema_version`, so the reader must accept its absence as version one for one release.

**Verifiably resolved when.**

- A test round-trips a stored theme written with `schema_version` and one written without it, and both load.
- A test asserts a document carrying an unrecognised `schema_version` is skipped with a notice, not an exception.
- `docs/formats/stored-theme.schema.json` exists and validates every theme the importer writes.
- The full project check is green.

---

## `F-19 (no fix request — pre-existing)` — P2 (F-19)

Route `gated_auto` -> owner `review-fixer` · session `talaria-w2`

**Exact paths.**

- `talaria/cli.py`
- `tests/test_cli.py`

**What is wrong.**

**Assigned to `talaria-w2` by the coordinator.** This is the shared entry point's error convention and it is pre-existing, so it belongs to no child by authorship. It is routed here because this session was the last writer of `talaria/cli.py` and just added the `theme import` subcommand there, so it holds the current shape of that surface.

`talaria/cli.py:210-228` dispatches to every verb with no surrounding `try`/`except`. A grep of the file finds handlers only for `ThemeImportError`, `CredentialError`, `KeyboardInterrupt`, `AdminError` and `RefreshError` — none for `ConfigError` or `StoredThemeError`. The result is a Python stack trace and Python's default exit code 1, rather than the deliberate exit-2-with-one-line convention every other operator error in this file uses at `:242`, `:371`, `:384`, `:799` and `:869`. Exit 1 is the code the file reserves for the gate's fail verdict at `:936`, so "the gate failed" and "your config file has a typo" are indistinguishable by exit code.

`talaria/config.py:150` and `:152` already raise `ConfigError` with operator-ready messages that are never printed. `docs/configuration.md:95-96` documents the intended behaviour as a launch error that names the offending file, which the traceback satisfies only by accident.

Sequence this after `fix-f65bdcd773f6 (part B)` in this same brief, which removes the `StoredThemeError` escape at its source; this request is the backstop for `ConfigError`.

**What to change.**

Wrap the dispatch body of `main` in `except (ConfigError, StoredThemeError) as exc: print(f"talaria: {exc}", file=sys.stderr); return 2`, matching the existing convention.

**Verifiably resolved when.**

- A test writes a `config.toml` containing invalid TOML and asserts exit status 2 with a single stderr line naming the file, and no traceback.
- The gate's fail verdict still returns 1, so the two remain distinguishable.
- The full project check is green.

