# Doc review — the model picker, and closing v0.1 with the evidence it produces

**Target.** `docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md`

**Reviewed revision.** Working tree on `main`, uncommitted, at `fb0ce3d`. The plan, its spec and its
emitted workflow are all untracked at review time.

**Blocked.** No. Ten findings — two `P0`, four `P1`, three `P2`, one `P3` — all fixed in place at the
operator's instruction to fix everything rather than only the blockers.

**Related artifacts.** Spec `docs/plans/2026-08-06-model-picker-and-v0-1-closure-spec.json`; workflow
`docs/plans/2026-08-06-model-picker-and-v0-1-closure.workflow.js`; saga
`task-talaria-model-picker-v0-1-closure`. Origin: `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`.

## Readiness summary

The plan can drive implementation now; it could not have thirty minutes ago, and the reason is worth
recording. The operator chose option (b) for row 13 — `HERMES_DASHBOARD_SESSION_TOKEN` leaves the
credential precedence chain — and that decision converted U3 from a documentation edit into a Medium
code change across production code and seven test modules. The unit's own file list had not caught up
and named a test file that does not exist.

The two `P0`s are both "an agent following this literally does the wrong thing." The emitted workflow
was stale — it predated the decision, so `/work` would have run the *undecided* U3, halting on a
question already answered. And `KTD<n>` is numbered per document: U6 told an agent to verify "KTD7's
precedence chain", but this plan's KTD7 is about model cost confirmation, while the startup precedence
chain is KTD7 of a *different* plan. The codebase adds a third numbering of its own.

Every `path:line` citation in the document was checked against the file it points at, including the two
in Hermes, which turned out to be verifiable locally and exact. One factual error was found and fixed:
`scope` is a body field on `ModelAssignment`, not a query parameter as the plan implied.

## Findings

| # | Priority | Finding | Status |
|---|---|---|---|
| D1 | `P0` | The emitted workflow predates the option-(b) decision — stale U3 label, prompt, return keys and file list, including the `HALT and ask` clause for a question now answered | fixed |
| D2 | `P0` | Cross-document `KTD<n>` collision: U6 cites "KTD7's precedence chain", but this plan's KTD7 is the expensive-model confirmation | fixed |
| D3 | `P1` | U1 and U3 run in the same wave and touch the credential story from opposite ends — a U1 fixture using the environment variable breaks when U3 merges | fixed |
| D4 | `P1` | The two new local commands are never named, and U2 and U4 both edit `commands.py` — two agents would invent two names | fixed |
| D5 | `P1` | Count error: the design overview says three new local commands; the units add two | fixed |
| D6 | `P1` | `scope=main` was written as a query parameter; it is a body field on `ModelAssignment`, and a wrong value is the HTTP 400 the plan's failure modes name | fixed |
| D7 | `P2` | `QUEUED.md:99` anchors to a section heading, not the sentence cited; line numbers in an edited journal are perishable | fixed |
| D8 | `P2` | `talaria/domain/models_catalog.py` sits beside an existing `models.py` with no stated reason not to merge them | fixed |
| D9 | `P2` | U6's dependency prose said "U1–U5" while the spec has `[U5, U3]` — and under option (b) the U3 dependency is real | fixed |
| D10 | `P3` | U2 "wires into `talaria/ui/app.py`" with no anchor, in a 113 KB file | fixed |

## What was fixed, and on what evidence

**D1.** Re-emitted the workflow from the corrected spec. Verified fresh: zero occurrences of
`test_credentials.py` and of the `HALT and ask` clause; U3's label now reads "Remove the environment
credential source and re-grade row 13".

**D2.** Added a note opening the Key Technical Decisions section: an unqualified `KTD<n>` means this
document's, a prototype-plan decision is written `KTD<n> (2026-08-02 prototype plan)`, and
`talaria/domain/models.py`'s docstring cites a third "KTD2" that is neither. The two cross-document
references — the startup precedence chain and the per-dial credential rule — are now qualified, and
U6 says outright that the KTD7 it means is not this plan's.

**D3.** U1's dependency line now carries the constraint. The units share no file, so nothing in the
spec's concurrency check would have caught this; it is a semantic collision, not a write race.

**D4.** Named `/models` (U2) and `/profiles` (U4), on evidence rather than preference: probed the live
gateway's catalogue — 114 command names, `/model` and `/profile` both **taken**, both singular;
`/models` and `/profiles` free. The plural/singular split is now stated as a deliberate hazard, since
an operator typing `/profile` reaches Hermes and `/profiles` reaches Talaria. Shadowing is already the
established pattern: the gateway also advertises `/quit`, and Talaria's local `/quit` wins.

**D5.** Two, not three. U5 adds no command — setting a default is an act inside the picker.

**D6.** Corrected against `hermes_cli/web_server.py:6533` in the local Hermes checkout: `profile` is a
query parameter, `scope` and `confirm_expensive_model` are body fields, and `scope` must be exactly
`"main"` or `"auxiliary"`.

**D7.** Cited by entry title instead of line number.

**D8.** Stated why the catalogue is a separate module: `models.py` is the JSON-RPC protocol boundary
whose argument turns on never coercing a malformation the transport must surface; the admin catalogue
is a different wire with a different failure vocabulary. An agent should not merge them.

**D9.** Prose now matches the spec and says why the dependency is real: U6 step (3) observes the
authentication-failure refusal on screen, and U3 rewrites that refusal's text. Observing the old
wording would record evidence for a Talaria that no longer exists.

**D10.** Named the six points where `PaletteRegion` is wired, and two hazards on that path:
`perform_local_command` is synchronous while `action_toggle_palette` is `async`, so a local command
cannot simply `await picker.toggle()`; and that dispatcher's docstring claims the control set is data
while its body is an `if`/`elif` chain, so adding a control does require an edit there.

## Also verified, and correct

`talaria/ui/palette.py:1-22` and `:16-21` — the anti-modal decision and the honest-degradation rule
are exactly at those lines. `talaria/transport/refresh.py` — `dashboard_origin_for`,
`_WEB_SCHEME_FOR`, `MAX_INDEX_BYTES` all exist. `talaria/transport/source.py` — `call()` writes the
outbound frame to the recorder *before* sending it, and its own docstring says so, which is what makes
U7's method enumeration a measurement rather than a recollection. `talaria --record` is a real flag on
the bare launcher, not only the `record` subcommand. Both Hermes citations are exact.

R29's two digest namespaces are stated and not conflated. No credential value, no operator profile
inventory, no local paths, no attribution lines. The `Clears:` backlink stays in its bracketed
placeholder form, and `tests/docs/test_gating_documents.py` passes (9 tests).

## Residual risk

Two things this review could not settle, both named in the plan rather than hidden.

Whether v0.1 reaches READY is still not knowable from here. U7 grades row 6 by enumerating which of
the thirteen required gateway methods the recordings prove were called; if the picker work does not
happen to exercise the remaining ten, the row does not clear and the verdict stays NOT READY on that
reason.

Option (b) is a behavior removal on a public repository's documented surface. The plan states exactly
how far it goes — no supported route places a credential in the environment — and explicitly forbids
the sentence that would overclaim it, since an inherited variable stays visible in `/proc/<pid>/environ`
whatever Talaria does. That boundary is the thing most likely to be quietly crossed during execution,
and it is why U7 owns the re-grading rather than U3.
