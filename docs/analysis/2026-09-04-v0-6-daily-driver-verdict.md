# Talaria v0.6.0 is **READY** as a daily driver

This document gates the v0.6.0 release the way the v0.1 verdict gated its
release: one fenced block states the verdict, and the evidence table below is
the authority behind it. The twelve rows are the ten manual-test findings plus
the credential hardening and the Hermes-touch annex, each graded against what
was actually run — the Gate-0 result on its tree, the fourteen receipts bound
to the candidate on theirs, and the automated suites that pin every row's
behavior. Gate-0 and the candidate live on different commits; the provenance
section below names both and shows why the result transfers.

```gate
id: v0-6-daily-driver
verdict: READY
review-by: 2026-09-30
```

## Evidence table

| Row | Condition | Status | Evidence |
| ---: | --- | --- | --- |
| 1 | `commands.catalog` models the supported response, no false drift | measured | Controller live row plus `tests/domain/test_commands.py`, `tests/transport/test_compat_baseline.py` |
| 2 | `/model` refusal surfaces honestly, no invented method | measured | Controller live row plus `tests/transport/test_commands.py` refusal test |
| 3 | Inspector toggle defaults to Ctrl+O, configurable, Herdr-usable | measured | Controller nested-toggle row plus `tests/ui/test_inspector.py`, `tests/ui/test_keybindings.py` |
| 4 | Ctrl+S cancels, footer distinguishes cancel from quit | measured | Controller idle-cancel row plus interrupt/footer/focus suites; XON/XOFF live protocol outstanding, documented as a limit |
| 5 | Routine seam rows live in the inspector, actionable stays visible | measured | Controller 4-live-rows row plus `tests/ui/test_inspector.py`, `tests/ui/test_status_region.py`, `tests/ui/test_live_wiring.py` |
| 6 | True-bottom multirow bar from versioned script documents, one runner | measured | Controller pickup row plus `tests/status/`, `tests/ui/test_status_bar.py` |
| 7 | Marketplace fetch, `/theme` select, explicit Reload, data-only | measured | Controller round-trip row plus `tests/ui/test_theme_import.py` incl. the same-slug repaint test |
| 8 | Homebrew fifth, listed but never startup-selected | measured | Controller row plus `tests/themes/test_homebrew.py` |
| 9 | Sparse inheritance, category/marker/background, offset reclaim | measured | Controller row plus `tests/themes/test_inheritance.py`, `tests/ui/test_transcript_bounds.py` |
| 10 | Enter on a picked entry executes exactly once | measured | Controller parity row plus `tests/ui/test_slash_palette.py` |
| 11 | Four provider credential names denied even when allowlisted | measured | `tests/status/test_env.py` synthetic-key denial plus the controller row |
| 12 | No Hermes touch anywhere in the v0.6.0 scope | met | No gateway code changed; compat baselines green; controller annex row |

## Candidate provenance — two trees, one product

Gate-0 ran at `c571590` (merge PR #136): uv sync clean, ruff clean, mypy
clean (201 files), pytest 2804 passed with 7 skipped, bandit exit 0, git diff
--check clean. The machine-readable Gate-0 record is
`docs/acceptance/v0.6.0/gate0.json`, which pins `candidate_commit` and `tree`
to `c571590`.

The fourteen receipts (twelve item plus two install-probe) and
`docs/acceptance/v0.6.0/artifact-manifest.json` bind to candidate `e967c20`.
Between the two trees the product is unchanged:
`git diff c571590 e967c20 -- talaria/ pyproject.toml uv.lock src/` shows
exactly one line, `__version__ = "0.5.0"` → `"0.6.0"`. The rest of the delta
is the release machinery itself (the version-agnostic workflow,
`scripts/acceptance/versioning.py`, additive v0.6.0 validators), one new test
file (`tests/docs/test_acceptance_versions.py`), and docs. The live matrix ran
in scratch against the candidate tree with existing sessions preserved and no
real secrets. No behavior change rides between Gate-0 and the candidate
binding, which is why the Gate-0 result transfers to the candidate.

## Limitations carried, not hidden

Bare host tty is unexercised. F2 slash.exec answers 4001 on listed sids (the
honest-unavailable leg stays the 4018). Marketplace fetch was not re-attempted
(prior evidence: search live, downloads refuse safely with 404). Corrupt-file
reload and sub-dialog races are unit-covered only. None of these blocks the
verdict above; each is recorded so a reader does not mistake coverage for it.
