# Code review — the model picker, and closing v0.1

**Target.** Branch `feat/model-picker-and-v0-1-closure` against merge base `6ce3754`.
**Reviewed SHA.** `621f0e6ae5e72cfee7851260de56ec98d35bb109`
**Mode.** programmatic / report-only, run as `/work`'s Phase 5 pre-PR gate.
**Scope.** 36 files, 6948 insertions, 209 deletions. Base fetched and confirmed not stale; no untracked
files excluded.

**Verdict.** Safe to merge. Two findings, both fixed before this artifact was written — one `P1`, one
`P2`. No `P0`.

**Related.** Plan `docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md`; spec
`docs/plans/2026-08-06-model-picker-and-v0-1-closure-spec.json`; doc review
`docs/reviews/2026-08-06-model-picker-and-v0-1-closure-plan-doc-review.md`; work session
`docs/work-sessions/2026-08-06-model-picker-and-v0-1-closure.md`; saga
`task-talaria-model-picker-v0-1-closure`.

## Findings

| # | Priority | Finding | Status |
|---|---|---|---|
| C1 | `P1` | The plan's KTD8 asserts option (b) means "no supported route puts one there", contradicted two lines above in the same section and by `credentials.py`, which now resolves the `TALARIA_GATEWAY_URL` token route **first**. Pre-existing on `main` (`6ce3754`), not introduced here, but it now contradicts the code this branch ships | fixed |
| C2 | `P2` | U6 wrote its checklist to `docs/plans/2026-08-06-u6-row19-operator-checklist.md`; the spec declares `docs/work-sessions/2026-08-06-live-acceptance-run.md`, which does not exist | recorded |

**C1** mattered because the plan is what every unit is told to read as its authority. After U3's work it
was the *only* document in the repository still asserting the false version — `DECISIONS.md` carries an
explicit "Not supported — do not write this anywhere" block, and the verdict's row 13 names the route as
"a surviving supported route that is an environment variable, and is the highest-precedence one." KTD8
now states the accurate claim and names the false one as false.

**C2** is location only; the content is complete. It is recorded rather than moved because the file is
already committed and referenced, and duplicating it would create two checklists. It matters more than a
normal path quibble: the checklist is the entire product of U6 — every other return is "pending operator
execution" — and the operator work it describes is the only thing that can clear row 19.

## Lenses run, and what each verified

**Credential handling.** `CREDENTIAL_HEADERS` is `("Authorization", "X-Hermes-Session-Token")` and
`_credential_headers` writes exactly those two keys, so the absence assertions cannot be evaded by a
third header. `AdminClient.__repr__` names only the origin, with the reason recorded in its own
docstring: the generated repr would render `self._provider`, the one object that can hold a live
credential. `_build_url` re-checks `(scheme, netloc)` after `urljoin` and raises `refused_origin`,
closing the redirect where an absolute path from a gateway response could retarget the credential.
`post_admin_json` runs the same `require_fetchable_origin` and `_build_url` discipline as the GET path —
the write surface is not a second, weaker one. No credential is ever placed in `params`; the only query
value is `profile`.

`admin.py` contains **no logging calls at all**, so credential-absence-from-logs holds structurally
rather than by assertion. The caplog tests still earn their place: they would catch a future logging
call that leaked the value.

**The environment-credential claim.** Swept the whole diff for any sentence asserting Talaria reads no
credential from its environment, or that R1's environment clause is met. Every match is a *negation* —
`DECISIONS.md`'s "Not supported — do not write this anywhere" block, row 13's grading, and the
work-session's correction. `tests/transport/test_process_surface.py::test_the_inherited_credential_is_visible_in_the_process_environment`
still asserts `surface.carries(CANARY_TOKEN)`, so `QUEUED.md`'s standing prohibition holds: R1's wording
was not widened, and the test still goes red only if some future Talaria genuinely scrubs its inherited
environment.

**The gate re-grade.** The `gate` block reads `verdict: NOT READY`, blocking on all three of row-19
unmet, row-13 partially unmet, row-6 inferred. Nothing cleared, and no `Clears:` backlink was written —
correctly, since a backlink would be a live claim the gating test checks. `tests/docs` passes (9 tests),
which is what enforces that the evidence table and the gate block agree.

Row 6's method enumeration was **re-derived independently** rather than accepted: parsing every
recording and grouping outbound frames by method reproduces U7's numbers exactly — 8 distinct methods,
`commands.catalog` 30 calls across 15 recordings, `session.most_recent` 15/15, `spawn_tree.list` 15/15,
`agents.list` 15/15, `delegation.status` 15/15, `session.create` 15/15, `prompt.submit` 18/12,
`slash.exec` 10/4. Ten of the thirteen evidence-only methods still have no runtime evidence. The prose
around the enumeration does not overstate it.

**Test quality.** The rotation guarantee was genuinely re-expressed, not weakened: the test writes
`token = "first"`, asserts it, rewrites the file with `token = "rotated"`, and asserts the new value —
two distinct literals through the surviving file route, so it cannot degenerate into comparing a value
against itself. That degeneration is the exact failure the diff's own `LEARNINGS.md` entry documents
happening elsewhere and being caught.

**Public-repo hygiene.** Checked every profile name on the operator's live gateway against the diff:
none appears. No attribution lines. R29's two digest namespaces are both present and correctly distinct
— `talaria-live-corpus-v1-2659f-…` for the aggregate, `talaria-live-v1-32f-…` for a single recording.

**Architecture.** `talaria/domain/models_catalog.py` and `talaria/domain/commands.py` import neither
Textual nor `talaria.ui`/`talaria.transport` (ADR-0002 holds). No local-file discovery anywhere in
`admin.py`, `models_catalog.py` or `picker.py` — no `provider_models_cache`, no `~/.hermes`, no
`Path.home()`, no `expanduser` (ADR-0001 holds). `POST /api/profiles/active` appears only in prose
explaining that it is never called (R3 holds).

Making `require_fetchable_origin` public is **justified, not a leak**. It guards one call with two
refusals — a scheme allowlist that keeps `file:` and `ftp:` away from `urlopen`, and a loopback rule
that keeps a cleartext credential on the machine it started on. A second copy for the admin surface
would be a second place for those to drift, and the copy that drifted would be the one that stopped
refusing. The docstring says exactly this.

**Concurrency.** The endpoint switch is serialized under `_switch_lock`. The reader is stopped *before*
the socket is dropped, with the hazard named in the code: left running, its `recv` would raise on the
closed connection and hand the drop to `_handle_disconnect`, starting a reconnect against whichever
target the switch had installed — two dialers racing for one connection slot. Every exit returns a named
`SwitchReport`, and state messages carry `target.safe_url` rather than a URL that could hold a token. On
a failed dial the source is deliberately left pointing at the requested endpoint; the docstring gives the
reason, so a retry goes where the operator asked rather than silently undoing it.

## Known deviations, assessed

Two were pre-declared and are sound: `talaria/transport/refresh.py` edited outside U1's file list (to
make `require_fetchable_origin` public for KTD1 — see above), and a local-command count in
`tests/domain/test_commands.py` plus four `tests/transport/` modules that U2's new entry changed.
Neither touches a file another unit owned.

U5 returned a junk key, `first_call_never_carries_confirm_expensive_model_expensive_model`, alongside
the real one. It left **no artifact in the code** — it exists only in the workflow return value, where
the schema's `additionalProperties: true` accepted it.

U6's path divergence is finding C2 above.

## Checks

Run locally on the reviewed SHA. GitHub Actions is in a major outage (incident opened 15:22Z, webhooks
throttled to roughly 15%), so no CI has run on this branch and local runs are the only signal.

| check | result |
|---|---|
| `uv run ruff check .` | exit 0 |
| `uv run mypy` | no issues, 115 source files |
| `uv run pytest` | 1274 passed, 1 skipped (up 179 from `main`) |
| `uv run bandit -r talaria -q` | exit 0 |
| `uv run pytest tests/docs` | 9 passed |
| `git diff --check` | clean |

## What this review could not establish

Whether the picker works against a live gateway. Every test here is local; U6's checklist is unexecuted
by design, and F7 — the gateway surviving Talaria's exit — cannot be settled by any frame log, because
the log ends at the exit being tested. The gate stays **NOT READY** for exactly that reason, which is the
honest outcome rather than a shortfall of this review.
