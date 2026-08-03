# Work session — Talaria v0.1, segment 1 (U1 scaffold, U4 fallback assessment)

Date: 2026-08-02
Plan: [docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md](../plans/2026-08-02-talaria-v0-1-prototype-plan.md)
Branch: `feat/talaria-python-scaffold`
Saga: `task-talaria-v0-1-prototype` (lifecycle phase `work`)
Backend: `cc-workflows-ultracode` (operator's recorded choice)

## What this session ran

This is an unattended `/work` run against the Talaria v0.1 prototype plan. The operator's launch
answers were pre-recorded in
[docs/plans/2026-08-02-talaria-v0-1-work-launch.md](../plans/2026-08-02-talaria-v0-1-work-launch.md).

Segment 1 carried two of the plan's ten units:

- **U1 — Python scaffold and quality gates.** Root `pyproject.toml` (uv-managed, `talaria` console
  script, ruff at 100 characters, mypy `--strict` on the package, pytest, bandit), `uv.lock`,
  `.python-version` pinned to 3.12, the `talaria/` package with `talaria/cli.py` implementing KTD7's
  startup precedence and `talaria/config.py` implementing KTD15's five-level precedence chain, the
  ADR-0002 boundary test, and a new `python-check` CI job.
- **U4 — Fallback presentation-layer assessment.** `prompt_toolkit` assessed against the same five
  criteria the U5 Textual validation gate measures, discharging PC8 and KTD12 and closing the
  QUEUED.md P0.

Commits: `94e04eb` (U1), `9b8b064` (U4).

## The one structural decision this session made

The plan's execution spec holds all ten units. The skill's ultracode path emits one workflow from
one spec, and that is what the committed workflow preview does.

This run **split the spec into three segments** instead — `.saga/segments/{seg1-scaffold,
seg2-milestone-1,seg3-milestone-2}.json` — derived by a script that copies each unit definition
verbatim from the canonical spec and prunes only the dependency edges an earlier segment already
satisfied. The script asserts that all ten units are covered exactly once, and all three segments
pass `execution_spec.py validate`.

The reason is a correctness gap, not scheduling taste. The emitted workflow's `__gate` function
checks only that a unit **returned** its declared keys; it never inspects the values. So a U5 unit
that returned `gate_verdict: "fail"` would have satisfied the gate and the run would have continued
straight into U7 — spending the entire live-transport milestone on a framework that had just failed
its validation gate. That is exactly the waste the plan's replay-first ordering exists to prevent,
and its "Unattended execution contract" forbids it in as many words: a U5 gate fail **halts** the
run. Splitting the spec at the gate is what makes that halt real rather than nominal.

The split also lands the three merge points on the cadence the contract already names: the U1
scaffold, the end of milestone 1 after the gate verdict, and the end of milestone 2 after the
daily-driver verdict.

## Checks run

Run by this session directly, on macOS arm64 (Darwin 25.5.0), uv 0.11.28, CPython 3.12.11 — not
taken from the unit's self-report:

| Check | Result |
| ----- | ------ |
| `uv run ruff check .` | clean |
| `uv run mypy talaria` | no issues found |
| `uv run pytest -q` | 9 passed |
| `uv run bandit -r talaria -q` | clean, exit 0 |
| `git diff --check` | clean |

Two of U1's stated verification bars were confirmed by falsification rather than by assertion:

- **`uv tool install --from . talaria`** installs a runnable `talaria`. `--help` exits 0;
  `--session X --resume` exits **2** with `--session and --resume are mutually exclusive`, which is
  KTD7's "conflicting flags are a usage error before any connection is dialed".
- **The ADR-0002 boundary check genuinely bites.** Adding a throwaway module under
  `talaria/domain/` that imports `textual` makes `tests/domain/test_boundary.py` fail with the
  imported `textual.*` modules named in the assertion; removing it makes the test pass again. The
  scratch module was not committed.

## Code review and the fixes it forced

The programmatic review of `9b8b064` returned **BLOCKED**: 0 P0, **2 P1**, 7 P2, 5 P3, scope clean,
no secrets or operator paths in the diff. Both P1s were reproduced independently before any fix was
written; both reproduced exactly as reported.

**P1 — the ADR-0002 boundary guard had a silent hole.** `pkgutil.walk_packages` will not descend
into a directory without `__init__.py`, but Python imports such a module anyway. A domain module
importing `textual` was therefore fully invisible to the one test that enforces ADR-0002. A second
defect in the same file read process-global `sys.modules`, so the check would have started failing
— and blaming the domain package — the moment `talaria/ui/` legitimately imported Textual in
segment 2. Fixed: filesystem walk, a companion test asserting every domain directory carries
`__init__.py`, and the import sweep moved into a subprocess. Falsified in three directions (see the
table below). Full mechanism in [LEARNINGS](../engineering-journal/LEARNINGS.md).

**P1 — the CI "required" flag gated nothing.** The matrix key named `required` only negated
`continue-on-error`; it did not make any check required. Verified against the GitHub API: `main`
has no branch protection (HTTP 404) and no rulesets (`[]`). So nothing stood between a red check
and `main` — in a run authorized to merge unattended. The misleading key is removed and the macOS
job now fails the run outright, but *blocking a merge* is a repository-settings change that cannot
be expressed in a workflow file. That half is deliberately left to the operator and recorded in
[QUEUED](../engineering-journal/QUEUED.md) — configuring branch protection unattended against a
check name that did not match exactly would deadlock every merge, including authorized ones. Until
it exists, the gate is enforced behaviorally: this run does not merge without observing the
required legs green.

Nine of the twelve P2/P3 findings were fixed in the same pass, because each one's failure scenario
lands in segment 2 and costs more to fix after U3, U5, and U6 build on it: environment values
coerced by the declared type rather than the shape of the string, `DEFAULTS` deep-copied so a
consumer cannot corrupt the built-in defaults process-wide, a malformed integer naming the
offending variable instead of raising a bare `ValueError`, malformed TOML naming the file, the test
isolation fixture promoted to a repository-wide `tests/conftest.py`, `Config` made genuinely
immutable rather than cosmetically frozen, mypy extended to cover `tests/`, and KTD7's startup
precedence pinned by tests that survive into the repository.

Two findings were recorded rather than fixed, both because they are decisions rather than defects:
the branch-protection half of the CI finding, and the trust boundary on repo-local
`.talaria/config.toml` — KTD15 ranks it above the operator's global config, and KTD5 makes
`status.command` executable, so U6 turns a precedence question into an arbitrary-code-execution
path. Changing a settled key technical decision mid-run is not this session's call; both are in
QUEUED at P1.

## Checks after the fixes

Re-run directly, same environment:

| Check | Before | After |
| ----- | ------ | ----- |
| `uv run ruff check .` | clean | clean |
| `uv run mypy` | 4 files (package only) | **9 files, tests included**, no issues |
| `uv run pytest -q` | 9 passed | **22 passed** |
| `uv run bandit -r talaria -q` | clean | clean |
| `git diff --check` | clean | clean |

Boundary-guard falsification, run three ways:

| Scenario | Result |
| -------- | ------ |
| `talaria/domain/models/decode.py` importing `textual`, **no** `__init__.py` | fails — "missing in: ['domain/models']" (green before the fix) |
| Same violation, `__init__.py` added | fails — names 43 imported `textual.*` modules |
| `textual` pre-imported into the pytest process, domain clean | **passes** — no false accusation |

## Opening the PR closed the CI gap, and exposed a third failure

`.github/workflows/validate.yml` triggers on push-to-`main` and on `pull_request`, so pushing the
feature branch alone started no run — which is why CI had never been observed green. Opening
[PR #10](https://github.com/infiquetra/talaria/pull/10) produced that evidence for the first time,
and the first run **failed**.

Not on this branch's code. TypeScript typechecked clean and all 45 vitest tests passed; the failure
was `prettier --check .` over ten markdown and JSON files under `docs/`. It had been failing on
`main` since `064967b`, so it predates this work — these commits added three more files to the pile.

`prettier --write` would have been the wrong fix. The doc-review artifact records
`target_sha256_after: 010ff5f6…` for the v0.1 plan, and the requirements reconciliation records
before/after hashes for its receipts; reformatting would have rewritten those bytes and invalidated
the hashes that prove the review gate was satisfied. Prettier belongs to the superseded `src/`
bootstrap (ADR-0004) and was never scoped to govern Python-era documentation, so `.prettierignore`
now excludes `docs/` instead. The plan's live sha256 was re-checked afterward and still matches the
doc-review's recorded hash exactly. Recorded in [DECISIONS](../engineering-journal/DECISIONS.md).

**The U1 verification bar is now met.** CI is green on the full matrix: `python-check (3.12)` and
`python-check (3.13)` on macOS arm64, `check`, and both informational Linux legs.

## Delta re-review

The re-review of the fix commits returned **CLEAN** — 0 P0, 0 P1. Both original P1s were confirmed
fixed, with the reviewer independently reproducing the green checks against a `git archive` extract
rather than this working tree. Two new P2s and three P3s were raised; all five are folded in above
rather than deferred, because each one's failure scenario lands in segment 2.

The most valuable finding answered a question worth asking directly: *is there still a way for a
domain module to import a presentation framework and stay green?* There was. `_FORBIDDEN_PREFIXES`
named `textual` literally, making the guard a **deny-list of one framework** — a domain module
importing `prompt_toolkit` passed cleanly. That is not hypothetical: U4, in this same branch,
assessed and recommended `prompt_toolkit` as the fallback for exactly the case where Textual fails
its gate. Adopting it would have silently disarmed ADR-0002's only enforcement. The guard is now an
**allow-list** — the domain may import the standard library and its own package, nothing else — so
it cannot go stale when the framework choice moves.

Also fixed: the test-isolation fixture redirected only the *global* config level while its own
docstring described the repo-local hole it left open (`load_config()` resolves `./.talaria/` against
`Path.cwd()`, and KTD15 designs that file for per-project status commands, so an operator having a
real one is expected, not exotic); `AGENTS.md` — the document `CLAUDE.md` tells contributors to read
first — still documented `uv run mypy talaria`, which was verified to report success on an injected
type error in `tests/` that `uv run mypy` catches; and the tuple/`MappingProxyType` consumer
contract is now documented, since immutability changed what callers get back.

One finding was recorded rather than fixed: integer settings are type-validated but not
range-validated, so `TALARIA_STATUS_INTERVAL_SECONDS=-5` and `paste_collapse_lines=0` both resolve.
The bound is a semantic property of the consuming unit, and inventing minimums the plan does not
specify is not this session's call. Queued at P2 for U6.

**A defect in the fix itself was caught by falsifying it.** The first allow-list attempt listed
`talaria` as an allowed *prefix*, and since `"talaria.tui".startswith("talaria.")` is true, it
permitted every subpackage in the project — the check would have passed a domain module importing
`talaria.ui`. Splitting the root package (exact match only) from allowed subtrees (prefix match)
fixed it. Re-falsified five ways afterward:

| Scenario | Result |
| -------- | ------ |
| domain imports `websockets` (installed dependency) | fails — `found: ['websockets']` |
| domain imports `textual` | fails — `found: ['rich', 'textual', 'typing_extensions']` |
| domain imports `talaria.tui` | fails — `violating ADR-0002: ['talaria.tui']` |
| domain imports `talaria.ui` | fails — `violating ADR-0002: ['talaria.ui']` |
| clean tree | passes |

Final local state: ruff clean, mypy clean over 9 files, **23 passed**, bandit clean,
`git diff --check` clean, `npm run check` exit 0.

## Next step

Merge PR #10 under the operator's pre-recorded authorization, confirming checks are green at merge
time rather than trusting the earlier run, then start segment 2 (U2 recorder, U3 domain core, U6
status runner, U5 Textual gate). A U5 `gate_verdict` of **fail** halts the run.
