# Work session — the model picker, and closing v0.1

**Plan.** `docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md`
**Spec.** `docs/plans/2026-08-06-model-picker-and-v0-1-closure-spec.json`
**Saga.** `task-talaria-model-picker-v0-1-closure`
**Branch.** `feat/model-picker-and-v0-1-closure`, off `6ce3754`
**Backend.** `cc-workflows-ultracode` (operator's pick; `recommend_execution_backend()` returned
`team-execution`, and the divergence is recorded on the saga rather than resolved silently).

## Where this stands

U1, U3 and U2 are built and committed as `49f9218`. U4 onward is running under the resumed workflow.
Nothing is on a pull request yet, and GitHub Actions is in a major outage, so every check reported here
was run locally.

## What the run actually did

Cost admission narrowed the run to one agent at a time. The spec declares `max_concurrent: 3`, but
`concurrency_governor._tier_width` scales the declared width by tier cost — baseline `sonnet:high` is
12, `opus:high` is 32, so `(3 × 12) ÷ 32 = 1`. The lease broker independently issued
`reservation_width: 1`. The plan's stated wave 1 (U1 and U3 together) therefore did not happen, which
also dissolved the U1/U3 credential collision the doc review had raised as `D3`: U3 could not race U1
because it started after U1 finished.

### U1 — the admin HTTP surface, and the credential form pinned to source

Built `talaria/transport/admin.py` and `talaria/domain/models_catalog.py`.

KTD2 was **confirmed with a correction**. Hermes's `_has_valid_session_token` accepts two header forms
and its own docstring ranks them, calling the `Authorization: Bearer` path legacy support for older
dashboard bundles. Talaria sends both headers rather than betting on either.

That decision nearly went the other way on bad evidence, and the near-miss is the more useful record:
`git log -S'X-Hermes-Session-Token'` in the installed Hermes checkout returned exactly one commit dated
today, which reads as "this header shipped hours ago." The checkout is shallow — `.git/shallow` exists
and `git rev-list --count HEAD` returns 1 — so the pickaxe was dating every line in the repository to
the only commit it could see, with no warning and exit status zero. Written up in `LEARNINGS.md`.

`refresh.py`'s `_require_fetchable_origin` was made public as `require_fetchable_origin` so the admin
surface reuses the origin derivation and the loopback refusal instead of restating them. **This file was
outside U1's declared list** — a deliberate deviation, made so KTD1 could hold, and recorded here rather
than left to be discovered in the diff.

### U3 — option (b), and the sentence it refused to write

`TOKEN_ENV_VAR` is gone from production code, the branch returning `Credential(source="environment")` is
deleted, and `"environment"` is removed from the `CredentialSource` literal so no code path can produce
that label.

**The unit answered its own first return as "Partly", and it was right to.** A `token` query parameter
on `TALARIA_GATEWAY_URL` survives by KTD8's explicit instruction, and after the removal it is the
*highest* remaining precedence level — and `TALARIA_GATEWAY_URL` is an environment variable. So the
accurate claim is "no supported route **requires** a credential in the process environment, and the
dedicated credential variable is gone", not "Talaria reads no credential from its environment."

The plan's own summary and PR #30's body both overstated this. `README.md`, the `credentials.py` module
docstring and `DECISIONS.md` now carry the accurate version, and that distinction is exactly what bounds
how far row 13 may be re-graded. U7 owns the re-grade and grades against what U3 recorded; U3 did not
touch `QUEUED.md` or the verdict document, verified by an empty `git diff --stat` on both.

The test that asserts the failure is unchanged and still asserts it —
`test_the_inherited_credential_is_visible_in_the_process_environment` still goes red if Talaria ever
scrubs its inherited environment, which is the standing prohibition in `QUEUED.md` honored rather than
worked around.

A second-order effect surfaced during the removal and is written up in `LEARNINGS.md`: deleting the top
level of a precedence chain promoted the level beneath it, and a test that distinguished two credential
sources by using two different literals collapsed into comparing a value against itself. It was
re-expressed against two surviving sources rather than deleted.

### U2 — the session model picker

`talaria/ui/picker.py` is a foldable region mirroring `palette.py`'s shape, not a modal overlay (KTD3 —
the same tradeoff the command listing already settled). The local command is `/models`, plural, because
the live gateway advertises 114 command names and owns singular `/model`.

## The halt, and what caused it

The first run halted at U4:

```
API Error: 400 tools.20.custom.input_schema.properties:
Property keys should match pattern '^[a-zA-Z0-9_.-]{1,64}$'
```

U4's return key `credential_refusal_surfaces_as_credential_unavailable_with_reason` was 65 characters
against a 64-character ceiling. `execution_spec.py validate --require-receipts` passed the spec every
time it was asked, including immediately before launch, because the limit belongs to the API's
tool-schema grammar rather than the spec's — the key becomes a JSON Schema property name two steps
downstream, so the first thing that can object is the model API at the moment the unit spawns.

Renamed to `credential_refusal_surfaces_as_unavailable_with_reason` (54 characters; the next-longest key
in the spec is 57, so nothing else was near the edge), spec re-validated, workflow re-emitted, run
resumed from `wf_e7e7524e-4c6` so the three completed units replay from cache. Committed as `0dd9358`
with the generalizable rule in `LEARNINGS.md`.

## Checks

Run locally on `49f9218`, all clean:

| check | result |
|---|---|
| `uv run ruff check .` | exit 0 |
| `uv run mypy` | no issues, 114 source files |
| `uv run pytest` | 1187 passed, 1 skipped (up 92 from `main`) |
| `uv run bandit -r talaria -q` | exit 0 |
| `git diff --check` | clean |

GitHub Actions is in a major outage (incident opened 15:22Z, webhooks throttled to ~15%), so no CI has
run on this branch and none will until it recovers.

## Deviations from the plan's file lists

Recorded rather than absorbed. `talaria/transport/refresh.py` (U1, for KTD1 — see above) and
`tests/domain/test_commands.py` plus four `tests/transport/` modules carrying a local-command count
that U2's fifth entry changed. All are consequential edits the units could not avoid; none of them
touches a file another unit owned.

## Next step

Collect the resumed workflow's results for U4–U7, settle the receipts through
`dispatch_settlement.py`, re-run the full local pipeline, then the Phase 5 code-review gate. Expect U7
to leave the verdict at **NOT READY** — row 6 is graded by enumerating which of the thirteen required
gateway methods the recordings prove were called, and no new recordings exist yet.
