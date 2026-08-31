# Repair brief — issue #111: documentation and version

Reviewed revision: `122bd918e0056404e576ae5623ce9e97bfe1ad93` (the unmerged Talaria v0.5.0 integration candidate).
Diff base: `5efa19ccba31f84d8e732591b18767bf736d00c2`, the tip of `origin/main`.
Review outcome: `repairs_requested`. Full record: `docs/evidence/adhoc-orch-talaria-v0-5-0/artifacts/2b64a225506486bf59489bdefe3158ac95d5c8d1c9edebf9a985e60f303fdb1d.md`.

These requests come from one exact-revision Saga code review. Each is written to stand alone: you are not expected to have read the review. Work only in your own exclusive worktree, keep each commit scoped to one child issue, and do not repair a finding that belongs to another brief even if you can see it.

Requests labelled *(no fix request)* produced no entry in the typed `review_result.v1` fix-request list, because the consensus engine excludes pre-existing and advisory findings from consolidation. They are real repairs and are included so they are not lost. Requests labelled *(deferred — record, do not repair)* are debt entries: write the journal entry, change no code.

## Requests (8)

| Request | Severity | Findings | Route | Session |
| --- | --- | --- | --- | --- |
| `fix-fbd11376fc37` | P1 | F-5 | `safe_auto` -> `release` | — |
| `fix-ecc401dea49d` | P1 | F-8 | `manual` -> `release` | — |
| `fix-8a6b9d37c595` | P2 | F-15 | `safe_auto` -> `review-fixer` | — |
| `fix-69a7d0e9ddfa (documentation half)` | P1 | F-6 | `manual` -> `review-fixer` | — |
| `fix-7a7de5ca40c2` | P2 | F-34 | `safe_auto` -> `review-fixer` | — |
| `F-43 (no fix request — pre-existing)` | P3 | F-43 | `safe_auto` -> `review-fixer` | — |
| `debt-environment-allowlist (deferred — record, do not repair)` | P2 (deferred) | F-6 code half | `advisory` -> `release` | — |
| `debt-talariaapp-size (deferred — record, do not repair)` | P2 (deferred, advisory) | F-24 | `advisory` -> `release` | — |

**Ordering.**

- `fix-a8b6b76bb8cf` **must land before** `fix-ecc401dea49d`. The README and changelog rewrite quotes the acceptance verdict, so the acceptance record must already state the corrected verdict or the rewrite propagates a second stale claim.

---

## `fix-fbd11376fc37` — P1 (F-5)

Route `safe_auto` -> owner `release`

**Exact paths.**

- `docs/themes.md`
- `docs/configuration.md`
- `docs/engineering-journal/DECISIONS.md`
- `CHANGELOG.md`
- `tests/test_config.py`

**What is wrong.**

Four shipped documents tell operators that selecting an imported theme does not survive a restart and call it a known product defect. The shipped code does the opposite.

- `docs/themes.md:110-114` — "saving its slug does not survive the next restart ... This is a known product defect, not a configuration technique to work around."
- `docs/configuration.md:75` — "The four built-in slugs are accepted at startup" plus a pointer to the "current imported-theme persistence limitation".
- `docs/engineering-journal/DECISIONS.md:2803-2806` — recorded as a "Known implementation gap".
- `CHANGELOG.md:64-68` — the sole entry under "Known limitations".

`talaria/config.py:493-498` unions the discovered user-theme slugs into `available_theme_slugs`. Reproduced four times independently, including by the review controller: with an imported theme stored and its slug named in `config.toml`, `load_config` returns that slug with an empty notices tuple. No test references `load_user_theme_specs` or `available_theme_slugs`, so the working behaviour is unasserted and the false prose is ungated.

The operator consequence is that a headline feature of issue #105 will not be used, and a future maintainer will chase a defect that does not exist. The changelog occurrence is worse than the others: the "Known limitations" section invents this limitation while omitting the real ones.

**What to change.**

Replace the paragraph at `docs/themes.md:110-114` with the real contract — an imported theme's slug is accepted at startup once its canonical document exists under `<TALARIA_CONFIG_DIR>/themes/`, and the library is read once per process with no watcher. Correct `docs/configuration.md:75` to say built-in slugs **and** stored imported slugs. Supersede rather than delete the `DECISIONS.md` entry. Delete the `CHANGELOG.md` bullet and replace the section with the limitations that are real at this revision: the acceptance verdict, the pre-existing `growing-one-column-table` gate exceedance already in `QUEUED.md`, and the unchanged v0.4.0 limits.

**Verifiably resolved when.**

- A new test in `tests/test_config.py` writes a stored theme and asserts `load_config` resolves its slug with an empty notices tuple, so the corrected prose is gated. It must pass on current code — this one pins working behaviour rather than reproducing a defect.
- `grep -rn 'known product defect\|persistence limitation\|Known implementation gap' docs/ CHANGELOG.md` returns nothing referring to imported themes.
- The `CHANGELOG.md` "Known limitations" section names only limitations that hold at this revision.
- The full project check is green.

---

## `fix-ecc401dea49d` — P1 (F-8)

Route `manual` -> owner `release`

**Exact paths.**

- `README.md`
- `CHANGELOG.md`

**What is wrong.**

The release's front door and its changelog both tell a reader the acceptance run has not happened. It happened, and its recorded verdict in this same commit is **NOT SATISFIED**.

- `README.md:9` — "its release acceptance evidence is still outstanding"
- `README.md:60-61` — "The v0.5.0 acceptance run is still outstanding"
- `README.md:119-120` — "the v0.5.0 candidate has not completed its acceptance run"
- `CHANGELOG.md:15` — "the release's terminal acceptance evidence is not yet recorded"

`docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md` states NOT SATISFIED twice, once as Status and once as Final verdict, and the same commit adds fifteen receipts, twenty-two screenshots, twenty-three pseudo-terminal results and two install receipts.

"Still outstanding" reads as absence of evidence; the truth is evidence of failure. The two are not interchangeable for someone deciding whether to ship, and the error runs in the direction that favours shipping.

**What to change.**

Replace the "still outstanding" phrasing at all four locations with the recorded outcome and a link — for example: "The v0.5.0 acceptance run was executed on 2026-08-31 and returned NOT SATISFIED (see the acceptance results). The installed artifact was proven; the live primary model route failed and the `talaria-t1` item receipts and screenshots do not exist." Keep the existing v0.4.0 limitation list unchanged.

**Verifiably resolved when.**

- `grep -n 'still outstanding\|not yet recorded' README.md CHANGELOG.md` returns nothing about v0.5.0 acceptance.
- Each replaced sentence links `docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md`.
- The wording matches whatever verdict the record carries after fix-a8b6b76bb8cf lands — sequence this second.

---

## `fix-8a6b9d37c595` — P2 (F-15)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `docs/themes.md`

**What is wrong.**

`docs/themes.md:99-101` says "Talaria normalizes the result to a lowercase hyphenated slug". It does not normalize; it validates. `talaria/themes/__init__.py:77` defines the slug pattern and `:97` raises when a name does not match, and there is no normalizing transform on the path. Running the shipped verb: `talaria theme import theme.json --name 'Solar Flare'` prints `talaria: theme import failed: invalid theme slug: 'Solar Flare'` and exits 2.

`docs/themes.md` is the document a reader reaches first, from `docs/00-index.md:20`, and `docs/formats/vscode-theme-import.md:120-122` states the correct rule, so the two user documents contradict each other on the same behaviour.

**What to change.**

Replace the normalization sentence with the validation rule — the resulting name must already be a lowercase hyphenated slug, and a name with spaces, uppercase letters or path separators is rejected before anything is written — and show the observed rejection line as the example.

**Verifiably resolved when.**

- `docs/themes.md` and `docs/formats/vscode-theme-import.md` state the same rule.
- The example in `docs/themes.md` is a command and output pair that reproduces exactly.
- The full project check is green.

---

## `fix-69a7d0e9ddfa (documentation half)` — P1 (F-6)

Route `manual` -> owner `review-fixer`

**Exact paths.**

- `docs/configuration.md`

**What is wrong.**

`docs/configuration.md:94` states a blanket safety property: "Invalid optional values use the documented fallback and add a visible startup notice." It is true only for the `theme`, `ui` and `status` tables. `_normalize_config` at `talaria/config.py:267-317` never inspects the `composer`, `environment` or `profiles` tables.

Reproduced by the review controller: a config containing `composer.paste_collapse_lines = "six"` and `environment.allowlist = 42` loads with `notices = ()` and hands back the raw values `'six'` and `42` unchanged. With a valid `status.command` alongside the non-list allowlist, `talaria/cli.py:270-274` raises `TypeError: 'int' object is not iterable` — a launch crash where the document promises a bounded fallback.

The document's assurance is exactly what would stop a reader double-checking those rows.

**Split note.** The code half of this finding — hardening `talaria/cli.py:270` so a malformed allowlist degrades to default-deny — was **deferred to debt** by the coordinator, because `environment.allowlist` predates v0.5.0 and repairing it is scope expansion. It appears in this same brief as `debt-environment-allowlist`. Write the document against the behaviour that ships, not against the hardened behaviour.

**What to change.**

Narrow the sentence at `docs/configuration.md:93-96` to the tables that are actually normalized, and say plainly what the others do: the `theme`, `ui` and `status` rows are normalized once after precedence resolves and an invalid value there uses the documented fallback with a visible startup notice; a malformed `composer` threshold is silently replaced by its default at launch, a malformed `profiles` entry is silently dropped, and a malformed `environment.allowlist` is either a launch error or is read character by character, depending on its type. The measured cases are in `debt-environment-allowlist` below — use them, do not paraphrase the crash as the only outcome.

**Verifiably resolved when.**

- The sentence names exactly the three normalized tables.
- The document's statement matches what the reproduction above actually produces — re-run it after editing.
- The `environment.allowlist` sentence covers the string case, which does not raise.
- The full project check is green.

---

## `fix-7a7de5ca40c2` — P2 (F-34)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `docs/00-index.md`
- `README.md`
- `CHANGELOG.md`

**What is wrong.**

The one document stating whether this candidate passed is unreachable by following any link. `docs/00-index.md:19-24` lists the four new user guides and then jumps to "Analysis and evidence" at line 26 with no v0.5.0 acceptance, run-plan or visual-specification entry, and neither `README.md` nor `CHANGELOG.md` links the acceptance record. A grep for the v0.5.0 acceptance paths across those files returns only v0.4-era paths.

The run plan's documentation deliverable at line 859 names "acceptance link" explicitly. A link checker over all fourteen in-scope documents found zero broken links, so this is an absent link rather than a broken one.

**What to change.**

Add a "Release evidence" subsection to `docs/00-index.md` linking `acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md`, `acceptance/v0.5.0/evidence/t2/README.md`, `design/2026-08-30-talaria-v0-5-0-visual-spec.md` and `plans/2026-08-30-talaria-v0-5-0-run-plan.md`, and add the acceptance-results link to the README status banner and the changelog preamble.

**Verifiably resolved when.**

- The acceptance record is reachable in one hop from `docs/00-index.md`, from `README.md` and from `CHANGELOG.md`.
- Every added link resolves — re-run a link check over the documentation set.
- The full project check is green.

---

## `F-43 (no fix request — pre-existing)` — P3 (F-43)

Route `safe_auto` -> owner `review-fixer`

**Exact paths.**

- `CHANGELOG.md`

**What is wrong.**

`CHANGELOG.md:11` is `## [Unreleased]`, bracketed like every other version heading, but the reference definitions at `CHANGELOG.md:405-408` cover only the four released versions. The heading renders as literal text and gives a reader no route to the diff the section describes. `CHANGELOG.md:7` declares Keep a Changelog conformance, which specifies a compare link for exactly this heading.

Marked **pre-existing**, so the consensus engine raised no fix request for it; it is included here because it is a one-line edit in a file this child is already opening.

**What to change.**

Add `[Unreleased]: https://github.com/infiquetra/talaria/compare/v0.4.0...HEAD` above the `[0.4.0]` definition.

**Verifiably resolved when.**

- Every bracketed heading in `CHANGELOG.md` has a matching reference definition.

---

## `debt-environment-allowlist (deferred — record, do not repair)` — P2 (deferred) (F-6 code half)

Route `advisory` -> owner `release`

**Exact paths.**

- `docs/engineering-journal/QUEUED.md`

**What is wrong.**

**The coordinator deferred this deliberately: record it as debt, do not repair it.** `environment.allowlist` predates v0.5.0 and belongs to none of issues #104 through #111, so repairing it is scope expansion against a run contract that names scope expansion as a stop condition. Do not write code for it. Write the journal entry.

The behaviour, measured at the reviewed revision, is not quite what the review first reported, and the entry should record the corrected version:

- `environment.allowlist = 42` raises `TypeError: 'int' object is not iterable` at launch.
- `environment.allowlist = true` raises `TypeError: 'bool' object is not iterable`.
- `environment.allowlist = "FOO"` does **not** raise. `talaria/cli.py:270-274` reads `cfg.get("environment", "allowlist", default=[]) or []` and then does `tuple(str(name) for name in allowlist)`, so a string is iterated character by character and the runner receives `('F', 'O', 'O')` — a garbage allowlist, silently, with no notice.

The failure direction is restrictive rather than permissive: single-character names match no real environment variable, so the status command runs with an effectively empty allowlist rather than a widened one. That is why this is debt rather than a safety defect.

**What to change.**

Add one dated entry to `docs/engineering-journal/QUEUED.md` recording: the three measured behaviours above with `talaria/cli.py:270-274` cited; that `_normalize_config` at `talaria/config.py:267-317` covers only the `theme`, `ui` and `status` tables, so `environment`, `composer` and `profiles` values reach their consumers unvalidated; and that `docs/configuration.md` has been narrowed in this same release to stop promising otherwise (see `fix-69a7d0e9ddfa` in this brief). Give it a concrete revisit condition — for example: revisit when a release next changes `talaria/cli.py`'s configuration reads, or when a second unvalidated table produces an operator-visible failure, whichever comes first.

Do not change `talaria/cli.py`.

**Verifiably resolved when.**

- `docs/engineering-journal/QUEUED.md` carries a dated entry citing `talaria/cli.py:270-274` and all three measured behaviours, including that the string case does not raise.
- The entry names a concrete revisit condition, not "someday".
- `git diff --stat` for this request touches only `QUEUED.md`.

---

## `debt-talariaapp-size (deferred — record, do not repair)` — P2 (deferred, advisory) (F-24)

Route `advisory` -> owner `release`

**Exact paths.**

- `docs/engineering-journal/QUEUED.md`

**What is wrong.**

**The coordinator deferred this deliberately: record it as debt, do not repair it.** The repair is a refactor, not a fix, and this release chose a different mitigation. Do not restructure `talaria/ui/app.py`. Write the journal entry.

`talaria/ui/app.py:1117` begins `TalariaApp`, and the file ends at line 6651 — roughly 5,534 lines and 181 methods in one class. All six feature children had to edit it, so the run could not parallelise on structure alone and added a human coordination mechanism, the shared-surface lease, to serialize edits. A process control standing in for a seam works only while a coordinator enforces it, and the next multi-lane release inherits the same bottleneck.

Stated fairly, and the entry must say so: **this diff did not materially worsen it.** The file grew +367/−66 against a 6,350-line base — about 4.7% — across 28 scattered hunks, and the substance of all six children went into new modules: `inspector.py`, `diff_viewer.py`, `status_bar.py`, `themes/` and `domain/changes.py`. The seams held.

**What to change.**

Add one dated entry to `docs/engineering-journal/QUEUED.md` recording the class size and method count with `talaria/ui/app.py:1117` cited, the measured growth this release added, and — stated plainly — that **the shared-surface lease was the mitigation this release chose and the underlying size is unaddressed**. Name the extraction that would address it: moving the slash-command dispatch table out of `TalariaApp` into its own module, since that is the axis the last two releases both grew along and issues #106 through #109 each added a verb. Give it a concrete revisit condition — for example: revisit before the next release that plans three or more parallel lanes against `talaria/ui/app.py`.

Do not change `talaria/ui/app.py`.

**Verifiably resolved when.**

- `docs/engineering-journal/QUEUED.md` carries a dated entry citing `talaria/ui/app.py:1117`, the line and method counts, and this release's measured growth.
- The entry states that the lease was the mitigation and that the size itself is unaddressed.
- The entry names the slash-command dispatch extraction as the candidate repair and a concrete revisit condition.
- `git diff --stat` for this request touches only `QUEUED.md`.

