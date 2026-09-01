# Repair brief — issue #104: theme token foundation (cycle 2)

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

## Requests (3)

| Request | Tier | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- | --- |
| `fix-a3e140bc463a` | 2 | P1, P3 | F-44, F-70 | `manual` -> `review-fixer` | — |
| `fix-a3273b13bd5d` | 3 | P3 | F-74 | `safe_auto` -> `review-fixer` | — |
| `F-69 (no fix request — pre-existing)` | 3 | P3 | F-69 | `safe_auto` -> `review-fixer` | — |

**Ordering inside this brief.** Take F-44 before F-70. Both edit `talaria/config.py` within forty lines of each other, and F-44 changes the signature of the helper F-70's code path calls. This is a soft dependency: it prevents a merge conflict, not a wrong result.

**What the cycle-1 repair traded away, so you do not oscillate.** Commit 7606812 added symlink resolution to `atomic_replace_bytes` deliberately, so that `save_theme` could follow a `config.toml` symlinked into a dotfiles repository. That behaviour is wanted and cycle 1 asked for it. **Do not repair F-44 by reverting `talaria/config.py:346`** — that reintroduces cycle-1 finding 17 and the next cycle will raise it again. Gate the behaviour per caller instead.

---

## `fix-a3e140bc463a` — Tier 2 — P1, P3 (F-44, F-70)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `talaria/config.py`
- `talaria/ui/theme.py`
- `tests/test_config.py`
- `tests/ui/test_theme.py`
- `docs/formats/vscode-theme-import.md`
- `docs/configuration.md`
- `docs/themes.md`

### F-44 — P1 — theme import writes through a symlink outside the config directory

**What is wrong.**

`talaria/config.py:346` now reads `target = path.resolve() if path.is_symlink() else path`. The pre-repair body did `os.replace(temp_path, path)` with no resolution. The helper has two callers. `save_theme` at `talaria/config.py:492` asked for the new behaviour. `write_user_theme` at `talaria/ui/theme.py:229` did not, and inherited it.

The consequence, reproduced by the review controller at this exact revision: plant a symlink at `<config>/themes/pwned.json` pointing at any file outside the config directory, run `talaria theme import --name pwned`, and the victim file is overwritten with theme JSON. The command exits 0. The target's mode is preserved, so nothing about the result signals a write happened. `os.path.islink` on the planted path still returns `True`.

Three lenses ran the same counter-probe independently: reverting only line 346 to `target = path` leaves the victim untouched and replaces the link. Cycle 1 confirmed that nothing could escape the configuration directory. That containment is gone.

Two supporting gaps: `tests/test_config.py:148` covers `save_theme` through a symlink and nothing covers `write_user_theme`; and `docs/formats/vscode-theme-import.md:151` promises "A later import of the same slug atomically replaces that file", which is false when the path is a symlink.

**What to change.**

Give `atomic_replace_bytes` an explicit `follow_symlinks` keyword defaulting to `False`. Pass `True` only from `save_theme`. Leave `write_user_theme` on the default, so a link planted in the theme directory is replaced rather than written through. Correct the helper's docstring, which still claims the temporary file is created in the same directory as `path` — after resolution it may not be. Bring `docs/formats/vscode-theme-import.md:151` into line with whichever behaviour each caller ends up with.

**Verifiably resolved when.**

- A test plants a symlink at `<config>/themes/<slug>.json` pointing at a file outside the configuration directory, runs the import path end to end, and asserts **both** that the outside file's bytes are unchanged **and** that `os.path.islink(<config>/themes/<slug>.json)` is now `False`. Asserting only one of the two passes for the wrong reason.
- A second test asserts `save_theme` still follows a symlinked `config.toml` and writes the link's target, so the cycle-1 behaviour is pinned rather than assumed.
- `grep -n 'follow_symlinks' talaria/ui/theme.py` returns nothing, or returns an explicit `False`.
- The full project check is green.

### F-70 — P3 — the theme writer refuses an inline theme table permanently

**What is wrong.**

TOML spells a top-level theme name three ways: a table header, a dotted key, and an inline table. The cycle-1 repair added the dotted branch at `talaria/config.py:470-476` and not the inline one, so `save_theme` against a configuration using the inline form raises an error whose message asserts there is no theme table — while the file plainly defines the name. The dotted pattern matches only an unquoted line-leading assignment, so the quoted spelling fails the same way.

Measured by the correctness lens: eleven other configuration shapes save successfully; the inline table and the quoted key both raise. Nothing corrupts, but the documented persistence route is unavailable and the diagnostic points at the wrong thing. Neither `docs/configuration.md` nor `docs/themes.md` states the limitation.

**What to change.**

Add an inline-table branch that rewrites only the `name` pair inside the braces, and let the existing reparse-equality check prove nothing else moved. If that is out of scope for this round, make the refusal honest: name the inline form in the error text and record the limitation in both guides.

**Verifiably resolved when.**

- Either a test saves a theme against an inline `theme = { name = "..." }` table and asserts the reparsed document differs in exactly the `name` value, or the error message names the inline form and both guides say it is unsupported.
- The quoted-key spelling is covered the same way, whichever route is taken.
- `uv run pytest tests/test_config.py -q` is green, then the full project check is green.

---

## `fix-a3273b13bd5d` — Tier 3 — P3 (F-74)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/themes/storage.py`
- `tests/test_config.py`

**What is wrong.**

`talaria/themes/storage.py:91` calls the writer's serialiser purely for its validation side effect. The writer legitimately has no path at that point, so its raise carries none — but every other error the reader emits begins with the offending path. Measured across all five malformed shapes: four skip notices name the file and the missing-tokens one names none. With several themes installed, the operator is told a theme was skipped and not which one.

This is the same shape as F-44 in the request above: a validator written for the write path is now binding on the read path, carrying the write path's message convention with it.

**What to change.**

Wrap the call at `:91` and re-raise with the path prefixed. Leave the writer's own message path-free.

**Verifiably resolved when.**

- A test loads a theme directory containing two stored themes, one of them missing a required token, and asserts the notice text contains the offending file's name.
- All five malformed shapes are covered by a parametrised assertion that each notice names the file.
- The full project check is green.

---

## `F-69 (no fix request — pre-existing)` — Tier 3 — P3 (F-69)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `talaria/themes/__init__.py`
- `docs/formats/stored-theme.schema.json`
- `tests/ui/test_theme.py`

**What is wrong.**

`talaria/themes/__init__.py:110-111` constrains the theme display name only to being non-empty after stripping, beside a strict slug pattern at `:108`. The published schema agrees, giving `name` a type and a minimum length with no pattern and no maximum, while `slug` carries a pattern. A stored theme therefore loads with a display name containing raw escape and bell bytes, bidi overrides, or unbounded length. The accessibility lens measured a stored theme whose name contains an escape sequence loading through `load_config` and the registry with no notice and the byte intact.

Every render path traced today defangs it, so nothing reaches the terminal raw. The containment rests on each future renderer remembering, rather than on the boundary that reads the file — which is the shape of the defect cycle 1 found in the importer. The security lens scored this at confidence 75 and did not prove the render paths exhaustive.

This is pre-existing: it predates the v0.5.0 work and the consensus engine therefore excluded it from consolidation. It is included so it is not lost.

**What to change.**

Reject the value where the file is read. In the theme record's post-init, refuse any name containing a C0 control character, DEL, or a Unicode format character, and cap the length. Mirror the constraint in the published schema. The domain package must not import the presentation defang helper (ADR-0002), so write a small local predicate.

**Verifiably resolved when.**

- A test loads a stored theme whose `name` contains `\x1b[31m` and asserts the load is refused with a named error, not that the rendered output is clean — a render-level assertion passes for the wrong reason.
- `docs/formats/stored-theme.schema.json` carries a `pattern` and a `maxLength` on `name`, and a validator run in the suite rejects the same document.
- `python -c "import talaria.themes, sys; assert 'textual' not in sys.modules"` still succeeds.
- The full project check is green.
