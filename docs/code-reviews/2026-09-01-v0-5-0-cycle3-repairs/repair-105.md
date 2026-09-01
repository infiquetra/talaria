# Repair brief — issue #105: Visual Studio Code theme import (cycle 3)

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
| `fix-d16bb32e4d26` | P3 | F-98 | `gated_auto` -> `review-fixer` | no — **preserves the receipts** | — |
| `fix-ff4bb42eaf9e` | P3 | F-100 | `safe_auto` -> `review-fixer` | no — **preserves the receipts** | — |
| `F-90 (carried — not in the sixteen)` | P2 | F-90 | `gated_auto` -> `review-fixer` | yes, but not in the round | — |

**Neither approved request touches `talaria/`**, so this lane is off the re-drive path entirely. You can land at any time without costing anybody a testing round.

**Hard ordering, and it reaches outside this brief.** `fix-ff4bb42eaf9e` edits `CHANGELOG.md`, which quotes the acceptance summary sentence that issue #110's `fix-6303809f1f55` regenerates. Wait for that to land, or you reintroduce the drift cycle 2 raised as F-46 and cycle 3 raised again as F-107.

**One routing note, said openly.** `fix-ff4bb42eaf9e` touches three release-facing documents that issue #111 owns as files. It is routed here because writing it correctly needs the importer's semantics — the `kind` vocabulary and the report schema — and that knowledge lives in this lane. Coordinate the changelog line with #111 rather than both editing it.

---

## `fix-d16bb32e4d26` — P3 (F-98)

Route `gated_auto` -> owner `review-fixer`.

**Exact paths.** `docs/formats/stored-theme.schema.json`, `tests/ui/test_theme_import.py`

**What is wrong.**

This is cycle-2 finding F-55 mirrored, not F-55 surviving. That finding was that the published schema was **stricter** than the loader — it rejected a document Talaria loads happily. That is genuinely repaired, and the interface-contract lens proved it by counter-probe: putting `schema_version` back into `required` on a disposable copy made the new test fail with the exact validation error.

What it found instead is the same seam failing the other way. `docs/formats/vscode-theme-import.md:159-160` tells a theme author that `stored-theme.schema.json` is **the normative schema**, so an author who validates a hand-written theme against it expects Talaria to load the file. Three classes of document pass that validation and are then skipped at startup:

1. **A display name of whitespace only.** The schema constrains `name` with `minLength: 1` plus a pattern banning control and format characters, but `ThemeSpec.__post_init__` at `talaria/themes/__init__.py:117` additionally requires `self.name.strip()` to be non-empty. Measured: a document identical to what `talaria theme import` writes but with `"name": " "` produces zero validation errors and is skipped with `theme display name must not be empty`. A non-breaking space behaves identically.
2. **A slug colliding with a built-in theme** — validates, skipped with `stored user theme cannot replace built-in 'refined-default'`.
3. **A file whose name does not match its own `slug` field** — validates, skipped with `filename does not match stored slug`.

The lens walked all 1,114,112 codepoints and found the schema's hand-written blacklist and the code's `unicodedata.category(c) == 'Cf'` test agree exactly, in both directions. So whitespace is the only character-class gap; the other two are rules the schema cannot express at all. And none of the display-name rules appears in prose: grepping the documentation for "display name", "128 character" and "control or format" returns nothing, so the schema is the only statement of them and it is the wrong statement.

**What to change.**

Two independent halves.

- **Tighten what the schema can express:** change the `name` pattern to require at least one non-whitespace character. That closes the only case the schema could have caught on its own.
- **State what it cannot:** follow the precedent this release already set on the sibling schema. `docs/acceptance/v0.5.0/artifact-manifest.schema.json:5` carries a root `description` naming its enforcer. Add the equivalent here — that document validity is necessary but not sufficient, and that `talaria/themes/storage.py` additionally requires the filename stem to equal `slug` and rejects slugs colliding with a built-in theme.

**Verifiably resolved when.**

- The existing validator test gains the whitespace-name case and asserts the schema now **rejects** it, so the two directions stay pinned together rather than drifting apart again.
- A test asserts, for each of the three classes, that schema validity and loader acceptance agree — either both accept or both reject. That is the property the finding is about.
- The root `description` names `talaria/themes/storage.py` as the additional authority.
- The full project check is green.

---

## `fix-ff4bb42eaf9e` — P3 (F-100)

Route `safe_auto` -> owner `review-fixer`. **Depends on issue #110's `fix-6303809f1f55`.**

**Exact paths.** `docs/themes.md`, `docs/releases/v0.5.0.md`, `CHANGELOG.md`

**What is wrong.**

This release adds `talaria theme import FILE --json`, which writes one versioned JSON object to standard output and nothing to standard error — `talaria-theme-import-report-v1` on success, `talaria-theme-import-error-v1` on failure. That is the interface a script or an SDK wrapper binds to, and it is the only machine-consumable command output the product gained in v0.5.0.

It is described in exactly one file: `docs/formats/vscode-theme-import.md:124-143`, a mapping-and-warning-rules reference. The three documents a person reads to learn what is new all print the synopsis `talaria theme import FILE [--name NAME]` — `docs/themes.md:96`, `docs/releases/v0.5.0.md:27`, and `CHANGELOG.md:28`. Square-bracket optional-argument notation implies the list is complete. Under Keep a Changelog, the Added section is the canonical place a new user-facing flag is announced, and `grep -n -- "--json\|machine-readable" CHANGELOG.md` returns nothing.

Measured by calling `build_parser()` on the frozen revision: `talaria theme import` accepts `[-h] [--name NAME] [--json] FILE`. Running the real command on the repository fixture with `--json` produced a 2.5 KB JSON object on stdout, zero bytes on stderr, exit 0; against a nonexistent path it produced the error object on stdout, zero bytes on stderr, exit 3.

An integrator reading the release notes would not know the flag exists and would parse the prose output instead — exactly the fragility the versioned report was built to remove.

**What to change.**

Change the synopsis in `docs/themes.md:96` and `docs/releases/v0.5.0.md:27` to `talaria theme import FILE [--name NAME] [--json]`, with one sentence saying `--json` writes a versioned machine-readable report to standard output and pointing at `docs/formats/vscode-theme-import.md` for its fields. Add a matching clause to the changelog's Added section at `CHANGELOG.md:28`.

**Verifiably resolved when.**

- A test asserts that **every option `build_parser()` defines for `theme import`** appears in the themes-guide synopsis. Derive the expected set from the parser, not from a list typed beside the assertion — the repository already has this pattern at `tests/test_config.py:692`, where fenced TOML in the guides is parsed and asserted against runtime defaults.
- The changelog names the flag in its Added section.
- Your changelog edit sits on top of `fix-6303809f1f55`, not under it.
- The full project check is green.

---

## `F-90 (carried — not in the sixteen)` — P2 (F-90)

Route `gated_auto` -> owner `review-fixer`. **Not part of the approved round** — pre-existing. It is the highest-severity carried item and worth the coordinator's attention.

**Exact paths.** `talaria/domain/redaction.py`, `src/record/redact.ts`, `tests/recorder/test_equivalence.py`, `tests/status/`

**What is wrong.**

Five of the six credential-name patterns in `talaria/domain/redaction.py:54` allow a prefix, so `PASSWORD`, `MY_PASSWORD`, `GITHUB_TOKEN` and `AWS_SECRET_ACCESS_KEY` are all denied. The API-key pattern is the exception: it anchors to the whole name — `^(api|private|public|access|secret|signing)[-_]?keys?$` — so it matches `API_KEY` but **not** `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `LANGCHAIN_API_KEY` or `LANGSMITH_API_KEY`, which are the ordinary spellings.

This release makes that predicate load-bearing for a new boundary. `talaria/status/contract.py` grew 258 lines here and documents that the credential-shaped-name deny outranks the operator allowlist. The security lens falsified that claim: with a hostile allowlist naming six secrets, exactly one was forwarded in full — `LANGCHAIN_API_KEY`. The module's own comment at `talaria/status/contract.py:107` acknowledges the blind spot and works around one symptom by matching `LANG` exactly rather than by prefix, which narrows one path without closing the predicate.

The same predicate gates recorder key redaction at `talaria/recorder/redact.py:336`, and this repository publishes six recorded frame logs under `docs/acceptance/v0.5.0/corpora/t1/`, so a Hermes frame carrying an `openai_api_key` field would be written through unredacted. No actual instance exists: the byte scan of all 1012 published files found zero key-shaped strings.

**Why it is not in the sixteen, and why it is worth raising anyway.** It is pre-existing, so the engine excluded it. But it touches `src/record/redact.ts`, which carries the same pattern list, and `tests/recorder/test_equivalence.py` asserts the two agree — so widening the pattern changes recorder output and **both sides must change together** or the equivalence proof breaks. That coupling is the reason to decide it deliberately rather than let it drift into a later release.
