---
title: The model picker, and closing v0.1 with the evidence it produces
type: feat
status: active
date: 2026-08-06
origin: docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md
---

# The model picker, and closing v0.1 with the evidence it produces

## Summary

Talaria gains a model picker in three steps — session model, profile, then that profile's default
model — built on a Hermes HTTP API that already exists and already answers with the credential
Talaria already holds.

Every step is developed through observed, recorded live sessions, which is exactly the evidence the
v0.1 daily-driver verdict is still waiting on. The plan therefore closes the gate as a product of the
feature work rather than as a separate chore, and settles the one blocking row that no amount of
testing can produce.

## Problem Frame

Talaria launches against whatever gateway and model the environment happens to point at. On
2026-08-06 that meant profile `default` on `gpt-5.5 (openai-codex)` — an expensive model on the wrong
profile — with no way to see the alternatives or change them without leaving the interface. Switching
already works (`/model <name> --provider <slug>` dispatches correctly today); what is missing is
*discovery* and *selection*.

Separately, `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md` holds v0.1 at **NOT READY** on
three conditions, declared in its own `gate` block: row 19 (unmet), row 13 (partially unmet) and row 6
(inferred). Rows 19 and 6 need live, observed sessions. Building the picker *is* dozens of live,
observed sessions. Doing them as one body of work is cheaper than doing them twice, and the alternative
— shipping a feature while the gate sits untouched — is how the verdict went stale the first time.

## Requirements

**R1.** Talaria discovers the available providers and models over the transport, never by reading a
file on the local filesystem. ADR-0001 makes Talaria a client that dials a gateway it did not launch,
possibly on another host; a local read silently returns the wrong machine's answer.

**R2.** The session model picker changes the model of the **running** session, using the `/model` slash
command that already works. It does not write configuration.

**R3.** The profile picker selects **which gateway Talaria dials**, and reconnects. It does not attempt
to retarget a running Hermes process, which `POST /api/profiles/active` explicitly cannot do.

**R4.** The default-model picker writes a profile's configured default through
`POST /api/model/set?profile=<name>`, and surfaces the gateway's expensive-model confirmation rather
than silently confirming it.

**R5.** No picker introduces a second focus owner in front of the composer. Whatever shape the picker
takes, it honors the constraint `talaria/ui/palette.py:1-22` records.

**R6.** The credential form used for HTTP calls is decided against Hermes source and cited, not
inherited from whichever form was tried first.

**R7.** A picker that cannot reach its data says so. A model list Talaria could not fetch, and a list
the gateway annotated with a warning, are both distinguishable on screen from "there are no models" —
the same honest-degradation rule `talaria/ui/palette.py:16-21` already applies to the command listing.

**R8.** Row 19's remaining branches are exercised deliberately and recorded: the two unrun startup
paths (`--resume`, `--session <id>`), the compatibility check's real on-screen output, the
authentication-failure branch, the absent-capability branch, and F1/F7 observed by a person.

**R9.** Row 13's precedence question is decided — option (b), `HERMES_DASHBOARD_SESSION_TOKEN` leaves
the credential precedence chain (KTD8) — written into `README.md`, and the verdict row restated to
match, without widening R1's wording, which `QUEUED.md` explicitly forbids.

**R10.** Every gateway method exercised during this plan's live runs is recorded against row 6's
thirteen, and the row is re-graded on what was actually measured — not on the fact that runs happened.

**R0 — on the citations in this document.** Two kinds appear. Paths under `talaria/`, `tests/` and
`docs/` are in **this repository**. Paths under `hermes_cli/` are in **Hermes**, which is a separate
project — Hermes installs a checkout under `~/.hermes/`, and both were re-read and confirmed exact
there on 2026-08-06: `hermes_cli/web_server.py:6533` is `@app.post("/api/model/set")`, and
`hermes_cli/web_routers/profiles.py:489` is `@router.post("/api/profiles/active")` carrying the
docstring this plan quotes. An agent should still treat line numbers in another project as perishable:
confirm by the symbol, not the number, and update the citation if it moved. No unit blocks on being
unable to open a `hermes_cli/` path — the live gateway answers the same questions.

**R11.** Recording corpora are cited by sha256 digest and frame count, never by local path (R29). The
single-recording namespace `talaria-live-v1-<n>f-<hash>` and the aggregate namespace
`talaria-live-corpus-v1-<n>f-<hash>` are not conflated.

**R12.** No profile name, profile path, or other operator-specific inventory from `GET /api/profiles`
appears in a committed test fixture, document, or commit message. This is a public repository.

**R13.** When a row is re-graded, the evidence table and the `gate` block are restated together —
`tests/docs/test_gating_documents.py` fails the suite otherwise — and the clearing work carries a
backlink in the `Clears: <gate-id>#<condition-id>` form, naming this document's gate and the row it
cleared. (Written here in the bracketed placeholder form on purpose: the check treats a concrete
`Clears:` line as a live claim, and an example in a plan is not one.)

## Key Technical Decisions

**On decision identifiers, before anything cites one.** `KTD<n>` is numbered per document, and Talaria
has more than one document using the notation — so a bare `KTD7` is ambiguous and the codebase's
docstrings cite a different set again. In this plan, an unqualified `KTD<n>` always means *this
document's*. A decision from the v0.1 prototype plan is written in full as
**`KTD<n> (2026-08-02 prototype plan)`**, and there are two of them below. Note in particular that
`talaria/domain/models.py`'s docstring cites its own "KTD2" — the frozen-dataclass wire boundary — which
is **not** this plan's KTD2.

**KTD1 — The admin API is a second transport surface, not a second transport.** A new
`talaria/transport/admin.py` extends the HTTP seam `talaria/transport/refresh.py` already established
(`dashboard_origin_for`, `_WEB_SCHEME_FOR`), reusing its origin derivation and its loopback refusal.
Rationale: that module already turns a gateway WebSocket URL into an HTTP origin and performs an
authenticated GET against it. A parallel implementation would be a second place for the
scheme-mapping and loopback rules to drift.

**KTD2 — The credential rides an `Authorization: Bearer` header on HTTP, and the choice is cited.**
The WebSocket credential is a query parameter because Hermes reads `ws.query_params` only, a protocol
fact `talaria/transport/credentials.py` already pins to source. The HTTP surface is a different reader
and must be pinned to its own source line before the code depends on it. U1 makes the citation; if the
source shows a query parameter is the supported HTTP form, U1 changes the decision rather than the
evidence.

**KTD3 — The picker is a foldable region, not a modal overlay.** `talaria/ui/palette.py:1-22` rejected
a modal search box on the grounds that it puts a second focus owner in front of the composer. The
model picker faces the identical tradeoff and takes the identical answer: a region that folds away,
driven from the composer, with selection by command rather than by a captured caret. Rejected: a
Textual `OptionList` overlay — better ergonomics in isolation, but it reopens a decision this
interface already made, and reopening it belongs in its own change with its own reasoning.

**KTD4 — Discovery is cached per connection epoch, and the cache dies with the connection.** The model
list is fetched once per connection and discarded on reconnect. Rationale: the list is a property of
the gateway, and a reconnect may be to a different gateway — that is precisely what the profile picker
does. A cache that outlives its connection is the stale-gating-document defect in a new place.

**KTD5 — The profile picker reconnects; it does not mutate the server's active profile.**
`POST /api/profiles/active` sets a sticky preference for *subsequent* CLI commands and gateways and
"does not retarget the already-running dashboard process"
(`hermes_cli/web_routers/profiles.py:489`). Talaria therefore treats `GET /api/profiles` as an
endpoint directory — `gateway_running` says which are dialable — and switching means dialing a
different endpoint. Talaria does not call the POST at all in this plan.

**KTD6 — One credential per endpoint, resolved at dial time, never cached across a switch.** Each
profile's dashboard mints its own token, and `<config_dir>/credentials` holds exactly one. The
provider already resolves per dial (`LoopbackTokenProvider.acquire`, `talaria/transport/credentials.py`,
called on every dial by the rule **KTD11 (2026-08-02 prototype plan)** establishes), so a switch that
changes the endpoint must also re-resolve the credential for it and
report `credential_unavailable` when it cannot — the failure state the transport already has. What
this plan does **not** do is invent a multi-credential file format; that is named in Scope Boundaries.

**KTD7 — The expensive-model confirmation is surfaced, never auto-confirmed.**
`POST /api/model/set` returns `{"ok": false, "confirm_required": true, "confirm_message": …}` for a
model its cost guard objects to (`hermes_cli/web_server.py:6533`). Talaria shows the message and
requires a second, explicit act. Rationale: the entire motivation for this feature is not spending
money by accident; a picker that passes `confirm_expensive_model` on the first call would defeat the
one guard already protecting against it.

**KTD8 — Row 13 is decided: option (b), the environment variable leaves the precedence chain.**
`QUEUED.md`'s entry *"R1's environment clause is unmet, and no change to Talaria can meet it"* names two
options in its **Do.** paragraph and does not choose. The operator chose **(b)** on 2026-08-06:
`HERMES_DASHBOARD_SESSION_TOKEN` is dropped as a credential source, leaving the `0600` file at
`<config_dir>/credentials`, a `token` on `TALARIA_GATEWAY_URL`, and the interactive prompt.

*What (b) buys, stated exactly.* Talaria stops reading a credential from its environment, so no
supported route puts one there. It does **not** make an inherited variable invisible: the kernel
snapshots the environment block at `exec` and `/proc/<pid>/environ` serves that snapshot for the
process's life, whatever Talaria does. What changes is that the variable is no longer a route Talaria
documents, depends on, or requires — an operator who unsets it loses nothing, which is what makes R1
satisfiable by operator action rather than impossible. Row 13 may be re-graded only that far; see U3.

*The constraint, unchanged.* R1's wording is not widened (`QUEUED.md`, "Do not"). The test asserting
the environment half fails measures a kernel fact, not the precedence chain, so it stays and stays
asserting a failure.

*Cost, acknowledged.* `QUEUED.md` sizes (b) as **Medium** — "removing the environment credential source
entirely" — against (a)'s Small. The variable is referenced across production code and roughly seven
test modules (enumerated in U3), and it is the variable Hermes's own dashboard publishes. The
recurring-setup objection is what `talaria refresh-credential` removed on 2026-08-04.

**KTD9 — Live evidence is scripted, not hoped for.** Row 19's branches are a checklist a person
executes, under `talaria --record`, in an isolated session — not a hope that ordinary development
happens to cover them. The authentication-failure branch is now trivially reproducible: a stale token
in the credential file yields a real HTTP 403, and `talaria refresh-credential` restores it.

## High-Level Technical Design

The feature adds one module to the transport, one region to the UI, and **two** local commands —
`/models` (U2) and `/profiles` (U4). U5 adds no command; setting a default is an act inside the picker.

**The two names, and why they are these.** Probed against the live gateway on 2026-08-06: its catalogue
carries 114 command names, including `/model` and `/profile` — both **singular**, both taken. `/models`
and `/profiles` are free. The plural/singular split is therefore load-bearing and slightly hazardous:
an operator typing `/profile` reaches Hermes, `/profiles` reaches Talaria's picker. That is a deliberate
choice, not an accident, and both listings mark the local pair with the existing `local` availability
marker so the distinction is visible. Shadowing is already the established pattern — the gateway also
advertises `/quit`, and Talaria's local `/quit` has always taken precedence.

```
talaria/transport/admin.py     GET  /api/model/options   -> ProviderCatalog
  (new, KTD1)                  GET  /api/model/info      -> current selection
                               POST /api/model/set       -> default-model write (U5)
                               GET  /api/profiles        -> endpoint directory (U4)

talaria/domain/models_catalog.py   pure decode of the above into frozen dataclasses
  (new, ADR-0002: no Textual import)

talaria/ui/picker.py           foldable region, same shape as PaletteRegion (KTD3)
  (new)

talaria/domain/commands.py     two new TALARIA_LOCAL_COMMANDS entries: /models, /profiles
  (extended)
```

**Why `models_catalog.py` is a new module and not part of `talaria/domain/models.py`.** The existing
module is the *protocol* boundary — the frozen dataclasses the gateway's JSON-RPC frames decode into,
whose docstring turns on never letting a coercing validator repair a malformation the transport must
surface. The admin catalogue is a different wire, decoded from HTTP responses on a different surface
with a different failure vocabulary. Keeping them apart keeps `models.py`'s determinism argument about
one thing. An agent should not merge the two.

Selection dispatches through paths that already exist: the session model picker composes
`/model <name> --provider <slug>` and sends it down the existing `slash.exec` route; the profile picker
resolves an endpoint and drives the existing reconnect.

## Implementation Units

### U1. The admin HTTP client and its credential form

Establish the second transport surface and settle KTD2 against Hermes source before anything depends
on it.

**Scope.** New `talaria/transport/admin.py`; new pure decode in `talaria/domain/models_catalog.py`.
Read-only endpoints only: `GET /api/model/options`, `GET /api/model/info`.

**First act, before code.** Read Hermes's HTTP authentication path and cite the source line that says
which credential form it accepts. Record the finding in the module docstring the way
`talaria/transport/credentials.py` records the WebSocket equivalent. If the source contradicts KTD2,
change KTD2 and say so in the journal.

**Failure modes to cover.** 401 (a credential the gateway refuses); 404 (a gateway too old to carry
the endpoint — this is the absent-capability branch R8 needs); a connection refused; a body that is not
JSON; a `providers` array that is empty; a provider entry missing `slug` or `models`; a response large
enough to need the same read cap `refresh.py` applies (`MAX_INDEX_BYTES`).

**Test scenarios.** `tests/transport/test_admin.py` — the origin is derived from the gateway URL and
never from a configured HTTP setting; the credential is attached in the decided form and appears in no
log, repr or exception message; 401/404/refused/non-JSON each become a named error rather than an
exception from `urllib`; an oversized body is refused at the cap. `tests/domain/test_models_catalog.py`
— a provider with no models, a provider marked `authenticated: false`, and an unknown extra field all
decode without raising.

**Dependencies.** None — but U1 runs in the same wave as U3, which is **deleting the environment
credential source**. No fixture, test or docstring U1 writes may supply a credential through
`HERMES_DASHBOARD_SESSION_TOKEN`, or it breaks the moment U3 merges. Use the credential file route or
an injected provider. This is the one place two concurrent units can collide without sharing a file.

### U2. The session model picker

The operator can see what is available and switch the running session's model.

**Scope.** New `talaria/ui/picker.py` (foldable region, KTD3); the `/models` local command in
`talaria/domain/commands.py`; wiring in `talaria/ui/app.py`.

**The seam in `app.py`, named** — the file is 113 KB and the agent should not hunt for it.
`PaletteRegion` is wired at six points, and the picker follows each: the import; the `Binding` in
`BINDINGS`; the `yield` in `compose`; the `query_one` property; the `async def action_toggle_*`; and the
`apply(...)` call that hands it fetched data. Two hazards on that path. First, `PaletteRegion` is opened
by a **function-key binding and no slash command at all** — the picker is opened by a command because
that is what was asked for, so it needs the command path *and* should take a binding for symmetry;
adding one without the other leaves the two regions inconsistent. Second, `perform_local_command` is
**synchronous** while `action_toggle_palette` is `async` — a local command cannot simply `await
picker.toggle()`, so the toggle is either sync or scheduled. Note also that this dispatcher's docstring
claims the set is data and "a fifth control should be a row in that data rather than an edit here",
while the body is an `if`/`elif` chain on `command.action` — adding a control does require an edit
there. Leave the docstring truthful.

**Behavior.** The region lists providers and their models, marks the current one, and visibly
distinguishes a provider the gateway reports as unauthenticated — selecting one of those is a
guaranteed failure and saying so first is cheaper than a round trip. Selection composes
`/model <name> --provider <slug>` and dispatches it through the existing `slash.exec` path, so the
gateway's own success and error text reaches the transcript unchanged (verified working 2026-08-06).

**Failure modes to cover.** The catalog was never fetched; the fetch failed; the gateway returned a
warning alongside a partial list (R7); the operator selects a model the gateway then refuses; the
connection drops between listing and selection, so the epoch the list belongs to is gone (KTD4).

**Test scenarios.** `tests/ui/test_picker.py` — the region renders providers and models with the
current selection marked; an unfetched catalog, a failed fetch and a warning-annotated catalog each
render as their own distinguishable line; selection produces the exact command string
`model <name> --provider <slug>`; the composer keeps focus throughout (R5); a selection made against a
stale epoch is refused rather than sent.

**Dependencies.** U1.

### U3. Remove the environment credential source (row 13, option (b))

Settle row 13, which no test run can settle. The decision is made (KTD8): option (b).

**Scope.** `talaria/transport/credentials.py` — delete the `TOKEN_ENV_VAR` branch of `_resolve` and
rewrite the two refusal messages that advertise it; `talaria/cli.py` — the record refusal names "two
routes" and must name the routes that remain; `README.md` — the supported credential routes and why
the environment is not one; the journal. `talaria/transport/credentials.py` keeps `TOKEN_ENV_VAR` as a
public name only if something still needs it; if nothing does, it goes, and `tests/conftest.py`'s
scrub is re-expressed against the literal.

**The decision, and its exact reach.** KTD8 states what (b) does and does not buy. The one sentence
that must not be written is that R1's environment clause is now *technically* met: an inherited
variable is still visible in `/proc/<pid>/environ`, and that is a kernel fact no code change touches.
What (b) supports is the narrower, true claim — **no credential route Talaria supports places a
credential in the process environment**, so unsetting the variable costs the operator nothing.

**The property that must survive.** KTD11 resolves the credential per dial, so a credential rotated
between dials is picked up without a restart (`tests/transport/test_attach.py`, the rotation tests).
Under (b) that property must survive through the file — `talaria refresh-credential` rewrites it and
the next dial reads it. A rotation test deleted along with its environment variable takes a real
guarantee with it; each one is re-expressed against the file, not removed.

**The call sites, enumerated** — this is a Medium unit, not a documentation edit, and the executing
agent should not discover the list by grepping mid-flight. Production: `talaria/transport/credentials.py`
(the `__all__` export, the constant, the `_resolve` branch, and the refusal messages at the
no-credential and no-terminal paths); `talaria/cli.py` (the record refusal's route list). Tests:
`tests/conftest.py`, `tests/transport/test_attach.py` (the largest by far — precedence, rotation,
priming, and refusal-message assertions), `tests/transport/test_process_surface.py`,
`tests/transport/test_reconnect.py`, `tests/recorder/test_command.py`, `tests/test_cli.py`,
`tests/test_config.py`. `talaria/status/contract.py`'s child-environment allowlist is **not** in scope:
its credential-shaped-name deny already outranks the allowlist, so it never forwarded the value.

**What U3 does not touch.** The verdict document and `QUEUED.md`. Row 13's re-grading, the `gate`
block, and the queued entry's retirement all belong to U7, which owns every gate-adjacent document so
the table and the block are restated exactly once, by one writer. U3's product is the decision, the
code, and the README; U7 grades what U3 produced.

**Test scenarios.** `tests/transport/test_attach.py` — a token present only in the environment now
produces `CredentialError`, and the message names the file route and the prompt rather than the
variable; the rotation guarantee is re-expressed against the file and still passes;
`tests/transport/test_process_surface.py`'s inherited-visibility assertion is untouched and still
asserts a failure.

**Dependencies.** None. Independent of the picker; sequenced early because a late credential change
would land under a picker that had already settled the question by accident.

### U4. The profile picker

The operator can see which profiles exist, which are dialable, and switch to one.

**Scope.** `GET /api/profiles` in `talaria/transport/admin.py`; the picker region gains a profile mode;
the `/profiles` local command; the endpoint-switch path through the existing reconnect.

**Behavior.** List profiles with their configured model and provider, marking those whose
`gateway_running` is false as not dialable. Selecting one resolves its endpoint, re-resolves the
credential for that endpoint (KTD6), and reconnects. Talaria never calls `POST /api/profiles/active`
(KTD5).

**Failure modes to cover.** A profile whose gateway is not running; a profile whose gateway is running
but refuses the credential (the common case, since each mints its own — this must produce the existing
`credential_unavailable` state with the reason on screen, not a hang); the switch failing partway,
leaving the previous connection closed and the new one unmade; a profile list that is empty.

**Test scenarios.** `tests/ui/test_picker.py` and `tests/transport/test_admin.py` — profiles render
with dialability marked; selecting a non-running profile is refused before any dial; a credential
refusal on the new endpoint surfaces as `credential_unavailable` with a reason; a failed switch leaves
Talaria in a named state rather than silently disconnected. Fixtures use **synthetic** profile names
(R12).

**Dependencies.** U1, U2.

### U5. The default-model picker

The operator can set the default model for a selected profile, for new sessions.

**Scope.** `POST /api/model/set` in `talaria/transport/admin.py`; the picker gains a "set as default"
act; the confirmation flow.

**Behavior.** Writes through `POST /api/model/set?profile=<name>`. Read the shape carefully: `profile`
is a **query parameter**, while `scope` and `confirm_expensive_model` are **body fields** on
`ModelAssignment`. `scope` must be exactly `"main"` or `"auxiliary"` — Hermes raises HTTP 400 with
`detail="scope must be 'main' or 'auxiliary'"` for anything else, which is the 400 the failure modes
below name. This plan writes `main`; `auxiliary` is out of scope. When the response
carries `confirm_required: true`, the message is displayed and a second explicit act is required before
resending with `confirm_expensive_model` (KTD7). The distinction that this affects **new sessions only**
and not the running one is stated on screen, because Hermes's own docstring says operators get this
wrong.

**Failure modes to cover.** `confirm_required` on the first call; a 400 for a bad scope; a write to a
profile that does not exist; a success whose effect is invisible in the current session (the thing the
on-screen note exists to prevent).

**Test scenarios.** `tests/transport/test_admin.py` — the confirmation round trip requires two distinct
acts and the first never carries `confirm_expensive_model`; a 400 becomes a named error. `tests/ui/`
— the new-sessions-only note is present whenever a default is written.

**Dependencies.** U4.

### U6. The scripted live acceptance run

Execute row 19's remaining branches deliberately, with a person watching, under `talaria --record`.

**Scope.** No production code. A checklist executed by the operator, its recordings digested, and the
verdict document restated.

**The checklist.** (1) Launch with `--resume` and with `--session <id>` and record which session each
lands in — neither has ever run against Hermes, so the startup precedence chain
**KTD7 (2026-08-02 prototype plan)** fixes (`--session <id>` beats `--resume` beats default-new) is
unverified live. This is *not* this plan's KTD7, which is about model cost confirmation.
(2) Capture the compatibility check's real on-screen output: how many of the five read-only probes come
back `present`, and whether `spawn_tree.list` refuses the fixture. (3) The authentication-failure
branch: run with a stale credential, observe the refusal on screen, restore with
`talaria refresh-credential`. (4) The absent-capability branch, reached through U1's 404 handling.
(5) F1 end to end in an isolated throwaway session. (6) F7: exit Talaria and confirm by observation
that the gateway is still serving — **this one cannot be automated and cannot be settled by any frame
log**, because the log ends at the exit being tested.

**Test expectation.** none — this unit's product is evidence, not code. Its output is recordings cited
by digest and count (R11) and a restated evidence table.

**Dependencies.** U5 (and through it U1–U4), so the runs exercise the finished paths, **and U3** — step
(3) observes the authentication-failure refusal on screen, and U3 rewrites the text of that refusal when
it removes the environment route. Observing the old wording would record evidence for a Talaria that no
longer exists. Step (6) depends on nothing in this plan and may run at any time.

### U7. Re-grade the gate on what was measured

Close the gate honestly, or state precisely what still holds it.

**Scope.** `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md` — rows 6, 13 and 19, the `gate`
block, and the "what would change this verdict" section; `QUEUED.md` retirements; the journal.

**Method.** For row 6, enumerate which of the thirteen required methods were actually called during
U1–U6's live runs and re-grade only those — a row that improves because runs happened, rather than
because methods were observed, is the defect this whole gate exists to catch. The enumeration is
mechanical rather than remembered: every outbound frame is written to the frame log before it is sent
(`talaria/transport/source.py`, `call()` records `request.to_frame()`, which carries `method`), so the
set of methods a run exercised is recoverable from its recording. Count distinct methods across the
corpus and grade against that set. For rows 13 and 19, grade against what U3 and U6 actually produced.
If a row does not clear, say so and leave the verdict NOT READY on that reason.

**Test scenarios.** `tests/docs/test_gating_documents.py` — the evidence table and the `gate` block
agree; no condition remains listed that the table now grades as cleared; every `Clears:` backlink
written by this plan's units names a condition the gate no longer blocks on (R13).

**Dependencies.** U6, U3.

## Risk Analysis & Mitigation

**The gate closes on the appearance of evidence rather than evidence.** This is the highest-severity
risk and the one the project has already been burned by twice. Mitigation: U7 grades row 6 by
enumerating methods actually called, and `tests/docs/test_gating_documents.py` already fails a gate
block that disagrees with its table.

**Row 13's decision gets made implicitly by the code.** If U1–U5 land first and the picker's HTTP path
quietly settles how credentials are supplied, U3 becomes a rubber stamp. Mitigation: U3 has no
dependencies and is sequenced early.

**The picker's model list goes stale across a reconnect.** Mitigated by KTD4 tying the cache to the
connection epoch — the same mechanism the correlator already uses.

**Operator profile inventory leaks into the public repository.** Mitigated by R12 and synthetic
fixtures; the plan itself names no profile but `default`.

## Scope Boundaries

**Out of scope — true non-goals.**

- Any change to `hermes-agent`. This plan is Talaria-only, by operator decision.
- Retargeting a running Hermes process, or calling `POST /api/profiles/active` (KTD5).
- A multi-credential file format. KTD6 re-resolves per endpoint with the existing single-value file;
  an endpoint→credential mapping is a format change with its own migration question.
- Auxiliary model slots and mixture-of-agents configuration. `GET /api/model/auxiliary` and
  `GET /api/model/moa` exist and are not used here; MoA rendering is already queued separately
  (`QUEUED.md`, P2, "Add MoA progress and fallback rendering").
- The superseded TypeScript tree under `src/`.

**Deferred to follow-up work.**

- Wiring a **local** ollama provider into Hermes so locally-served models become selectable. The
  operator's local server holds models Hermes cannot currently reach, because only the cloud-hosted
  provider is configured. This is Hermes configuration work and is explicitly deferred; file it in
  `QUEUED.md` when this plan lands.
- Command *completion* rather than listing (`QUEUED.md`, P2, "Command entry is a listing, not
  completion"). The picker may make this more attractive; it does not do it.
