---
title: Talaria v0.4 — the fleet turn: implementation plan
type: feat
status: active
date: 2026-08-16
origin: docs/brainstorms/2026-08-16-talaria-v0-4-fleet-turn-requirements.md
---

# Talaria v0.4 — the fleet turn: implementation plan

## Summary

Build the session registry, seam probes, and needs-you queue of the v0.4 requirements across
multiple concurrent gateway connections — one per configured profile endpoint — with the focused
session's shipped behavior preserved verbatim. The plan rests on a completed evidence pass against
the running Hermes gateway's own source (revision `7095e23eb`) that settled the connection-topology
question the requirements left open: session events are routed to the one client transport that
owns the session, never broadcast, and the gateway offers a poll surface that covers everything a
passive listener cannot see.

Three operator rulings recorded 2026-08-16 bind this plan: multi-profile connections are sized and
implemented now (the requirements' PC1 scope valve is not taken); activating a session another
client drives requires an explicit confirmation naming the detachment consequence, never silent;
and the needs-you queue answers approval requests inline through the existing single answer path
while every other prompt kind navigates to its original session.

## Problem Frame

The requirements document fixes the WHAT: registry as root object, probes that name absent seams,
one flat typed queue ordered by wait age, source-and-age and fire-and-observe as standing rules.
What it deliberately left to this plan: the connection topology (PC1), the row and lifecycle
vocabulary (PC2), the probe set (PC3), the queue surface (PC4), the replay gate restatement (PC5),
and the focus-churn repair (PC6). All six are closed below, each from cited evidence rather than
choice-by-default.

## Grounding Evidence

The load-bearing facts, each verified 2026-08-16 from a current source. Hermes source citations are
into the running install's checkout at `~/.hermes/hermes-agent` (revision `7095e23eb`), which the
gateway process on this machine executes; they are recorded here as the new pinned read that U1
formalizes. Talaria citations are into this repository at the merge of the requirements document.

- **Session events are scoped to the owning transport.** `write_json`
  (`tui_gateway/server.py:1637`) delivers an event frame carrying a `session_id` to the one
  transport stored on that session; only session-less events reach every connected peer, via
  `_broadcast_global_event` (`server.py:1691`). A 90-second passive listen on a bare authenticated
  connection observed `gateway.ready` and nothing else. A registry cannot be fed by ambient
  listening; the requirements' "does one socket observe background prompts" question is settled: no.
- **The poll surface covers the gap.** `session.active_list` (`tui_gateway/methods_session.py:986`)
  returns every live session of the gateway process with `status` (`waiting | starting | working |
  idle`), `last_active`, `started_at`, `message_count`, `model`, `preview`, `session_key`, `title`.
  `waiting` derives from a gateway-global pending-prompt registry covering every prompt kind
  (`server.py:8462`, `_session_pending_kind`). `sessions.changed` is broadcast to all peers with an
  empty payload on a signature watcher (0.5s check, 2.0s re-broadcast floor — `server.py:3736`);
  `session.reclaimed` is broadcast to all peers with `{session_id, stored_session_id, reason}` for
  the three backend reap paths `idle_timeout`, `lru_evict`, `ws_orphan_reap` (`server.py:889`).
- **Approvals are queryable and aimable cross-session; other kinds are not.**
  `approval.pending` (`tui_gateway/methods_prompt.py:1448`) returns a session's pending approvals;
  `approval.respond` (`methods_prompt.py:1480`) resolves by session key from any connection and —
  drift from the pinned read RR-28 — now accepts an optional `request_id`. `clarify.respond`,
  `secret.respond`, `sudo.respond`, `terminal.read.respond` take only a `request_id`, and no query
  method exposes pending non-approval prompts for a session another client drives.
- **`approval.pending` is not side-effect-free.** It resolves its session through `_sess`
  (`server.py:2500`), which warms the session's agent build. Aimed at a lazy idle session it starts
  an agent. It is only safe against a session whose agent is already live — a `waiting` session's
  agent is live by construction, since the agent raised the prompt, and so is a `working` session's,
  since it is mid-turn. (This bullet originally named `waiting` alone; see KTD11's amendment of
  2026-08-17, which widened the trigger on U1's finding that an approval-blocked session reports
  `working` rather than `waiting`.)
- **Attach is steal.** `session.activate` (`methods_session.py:1024`) rebinds the session's
  transport to the caller; `session.create`/`session.resume` set `"transport":
  current_transport()` the same way (`methods_session.py:110`). The displaced client receives no
  notification (`session.reclaimed` fires only for backend reaps). Focusing a session another
  client drives takes its event stream and silently blinds that client.
- **Both topology branches are wire-capable; neither sees other processes.**
  `session.create`/`session.resume` accept a `profile` parameter that re-binds the session's state
  home (`methods_session.py:42`, `:318`) — app-global remote mode is real at the running revision.
  But sessions other clients drive in *other* profile-gateway processes are invisible to this
  process's `session.active_list` either way. Reaching another profile's fleet means dialing that
  profile's gateway. Today's shipped Talaria topology is already one endpoint per profile from
  Talaria's own config (`talaria/config.py:197`, `talaria/transport/source.py:897`), and this
  machine currently configures none (`~/.talaria/config.toml` does not exist), so the connection
  inventory defaults to the single credential-file gateway.
- **Live listing shapes.** The running gateway's `session.list` row carries exactly
  `id, message_count, preview, source, started_at, title` — no profile field, no lifecycle field —
  confirming the requirements' dependency note at the live revision, not just the pin.
- **New protocol surface since the pin.** Three respond bridges the pinned read does not know:
  `preview.read.respond`, `window.read.respond`, `mcp.setup.respond` (`methods_prompt.py:1412`,
  `:1420`, `:1429`) — the queue's item type must admit prompt kinds it has never heard of.
- **The UI has exactly two honest reserved-layout homes.** The only widgets that claim height once
  and structurally cannot reflow the transcript are the `height: 1` HelpBar
  (`talaria/ui/app.py:793`) and the composer's fixed one-row notice line
  (`talaria/ui/composer.py:515`); the four auto-height regions inside `#body` are the proven
  reflow offenders. The v0.3 caret-row removal (unit B1) established the height-stability rule
  R16 cites.
- **The queue's raw material half-exists.** `PendingPrompt` already carries `opened_at` and `seq`
  (`talaria/domain/models.py:213`) — `prompt_view` drops both and hard-filters to the focused
  session (`talaria/domain/projection.py:464`, `:490`). `PromptRegion.focus_first_unanswered()`
  (`talaria/ui/prompts.py:995`) is implemented, tested, and bound to nothing. The existing
  `respond_live(..., session_id=...)` and `respond_to_prompt(..., session_id=...)` paths already
  accept a named non-focused session and verify the prompt belongs to that session; the queue must
  reuse that guard rather than relax or bypass it.
- **Activation hydrates approvals and clarifications — and only those.** The
  `session.activate` reply payload includes `pending_approval` and `pending_clarify` when they
  exist (`tui_gateway/server.py:8708-8711`, `_live_session_payload`); pending sudo and secret
  requests are not in the payload. So a confirmed attach recovers the two hydratable kinds into
  cards, and a sudo/secret announced only to the displaced transport fails visibly on the row
  rather than being invented.
- **Approval request ids are synthesized at the running revision.** `_ApprovalEntry` defaults a
  `request_id` (`tools/approval.py:2593`, `uuid4` setdefault) and `approval.respond` forwards an
  optional `request_id` — the drift from RR-28 recorded in the decision journal.
- **The focus-churn repairs already shipped.** `focus_session`'s current contract keeps `prompts`,
  `flushed_prompt_ids`, and `approvals_seen` across a switch and resets `withdrawn_approvals`
  (`talaria/domain/state.py:409` docstring), pinned by the 53 tests of
  `tests/domain/test_prompt_registry.py`. The two register entries describe the pre-repair tree;
  U4's job is keeping those pins green under the registry and extending refusal reporting, not
  re-fixing them.
- **The runner has a bare health route; the Kanban dispatcher has no route at all.**
  `GET /api/health` exists (`hermes_cli/web_server.py:3280`); no `/api/kanban` route exists at the
  running revision, so the `kanban-dispatcher` seam can only ever be never-observed in v0.4 —
  inventing a probe URL for it would repeat the `absent_capability` class of error.

## Requirements

The requirements document's R1–R25 are this plan's checklist; the plan mints no parallel numbering.
The unit assignment (every ID, no range compression):

| Requirement | Unit(s) | Requirement | Unit(s) |
| ----------- | ------- | ----------- | ------- |
| R1          | U3      | R14         | U6      |
| R2          | U4      | R15         | U6      |
| R3          | U3, U4  | R16         | U7      |
| R4          | U3      | R17         | U7      |
| R5          | U3      | R18         | U6, U7  |
| R6          | U4      | R19         | U6, U7  |
| R7          | U8      | R20         | U3, U4, U5, U6, U7, U8 |
| R8          | U2      | R21         | U6, U7  |
| R9          | U5      | R22         | U2, U4, U5, U7 |
| R10         | U5      | R23         | U4, U5, U7 |
| R11         | U5      | R24         | U3, U5  |
| R12         | U5      | R25         | U3      |
| R13         | U6      |             |         |

Three operator rulings of 2026-08-16 bind as additional requirements:

- **OP1.** Multi-profile connections are sized and implemented in v0.4. The PC1 scope valve
  (registry-over-the-configured-gateway) is not taken.
- **OP2.** Activating a session another client drives requires an explicit confirmation that names
  the consequence — the other client is detached. Never silent.
- **OP3.** The queue answers approvals inline through the single shipped answer path; every other
  kind navigates to its original session.

## Key Technical Decisions

**KTD1 — One concurrent connection per configured profile endpoint; the remote-mode `profile`
parameter goes unused.** The connection inventory is `[profiles.endpoints]` from Talaria's config
plus the credential-file gateway as the default entry; with no config file the inventory is that
single default, and today's shipped behavior is unchanged. Sessions of a profile with no configured
endpoint are out of reach and named as such (R10's named-absence rule applied to a whole profile).
Rejected: one app-global connection using the `profile` parameter on `session.create`/`resume` —
scoped event routing means it still could not see sessions driven by other clients, and other
profiles' gateway *processes* are invisible to it entirely; it collapses exactly where the fleet
matters. Rejected: the staged single-gateway valve — operator ruling OP1.

**KTD2 — Feed architecture: events for sessions Talaria drives, polls for everyone else.** Per
connection: full event streams feed the focused session and any session Talaria opened or resumed
(the v0.1–v0.3 machinery, unchanged); `session.active_list` polls — triggered by `sessions.changed`
(client-side coalesce 2s, matching the server floor) with a 30-second periodic backstop — feed
every other row; `approval.pending` fetches per-item detail for rows reporting `waiting` or
`working` (KTD11 as amended by the operator ruling of 2026-08-17);
`session.reclaimed` retires rows; `session.list` re-lists on reconnect and on the same coalesced
`sessions.changed` hint, epoch-paired with the `active_list` poll, and a failed listing marks
listing-derived fields stale (never cleared). A foreign session waiting on a non-approval kind
renders as a typed queue item of kind `unobserved` (the gateway exposes only the flattened
`waiting`), resolved per AE11's amended rule: navigation goes through the KTD8 confirm — whose
copy states the kind is unknown and may not be recoverable — and an attach that hydrates no card
**latches a visible resolved-failed** on the row and the item, the terminal-read settle precedent
generalized. Every path is keyboard-reachable; none ends silently. The three running-revision kinds Talaria has no card for
(`preview.read`, `window.read`, `mcp.setup`) are named on the registry row and are **not** queued:
R14 enumerates the four queueable kinds, and the queue holds only resolvable items — an item whose
kind Talaria cannot render anywhere is not resolvable.

**KTD3 — `FleetState` wraps the existing `TalariaState`; the focused engine is not rewritten.** The
new root object holds the registry, the queue, per-connection state, and the focused cursor; the
existing `TalariaState` remains the focused session's reducer, fed exactly as today (R2's
guarantee by construction, minimal blast radius). The registry key is `(profile, durable_id)` —
`session_key` when known, else the listing `id` — because the repo's recorded two-identities
decision means runtime ids change across resume; runtime ids are routing aliases rebound on
resume, never the key. The profile half is the identity of the observing connection (no listing
row carries a profile field, verified live), so a stored id seen on two connections is two rows. The cross-talk guard's
*protection* survives inside the focused engine; its *discard* is replaced one level up, where the
router now sends foreign-session events to registry rows (R4). `cross_session_events_ignored`
stops counting routable events and remains only as the identity-less counter R25 requires.

**KTD4 — The protocol baseline re-pins to the running revision `7095e23eb`, with probed absence.**
v0.4 builds on methods the old pin (`7f4d15515`) does not have (`session.active_list`,
`approval.pending`). U1 records the new pinned read; the probe set treats both methods as probed
capabilities, and a gateway lacking them gets fleet features off by name ("roster unavailable:
session.active_list absent") while the single-session core keeps the old-pin baseline. Rejected:
building a degraded roster on `session.list` polling — it has no lifecycle field, so the degraded
mode would fabricate exactly the zeroes R10 exists to prevent.

**KTD5 — The credential store extends to one entry per profile, backward-compatibly; endpoints
have exactly one source.** The existing top-level `token`/`url` pair stays the default profile's
entry; `[profiles.<name>]` tables add `token` only — endpoints live solely in config
`[profiles.endpoints]`, so there is never a second place an address can come from (panel finding:
a per-entry `url` override created precedence nobody defined). Two configured profiles whose
endpoint URLs parse equal share one connection only when their resolved tokens are also
byte-identical; otherwise the launch refuses loud and names both profiles — never a silent pick
of one credential for another profile's gateway. The canonical connection identity is the first
configured profile name in config order, and it is the name frames and registry rows carry.
Mode-0600 discipline, per-dial re-read, and the no-environment-variable rule are unchanged. A
TOML document that fails to parse fails the whole file loud — one document has no per-entry
syntax boundary; per-profile isolation applies to semantic validation after a successful parse
(a missing or malformed table refuses that profile's connection and names it, leaving the rest
usable). `talaria refresh-credential` gains `--profile <name>`. Acquisition for a profile never
configured fails loud with the pairing instruction, never falls back to another profile's token.

**KTD6 — Recording stays one frame log; a multi-connection log stamps per-frame `profile` keys, a
header `connections` list, and header version 2; a single-connection log stays version 1
byte-identical.** `docs/formats/frame-log.md`'s rule is that the version bumps "when a reader
must notice a change" (`frame-log.md:34`) — and a multi-connection log is exactly that case: a
version-1-only reader that ignored the `profile` key would silently merge equal session ids from
different gateways into one wrong fleet, so the bump makes an old reader notice rather than
misread. A single-connection run emits no `profile` keys, no `connections` list, and version 1,
so v0.1–v0.3 logs and the TypeScript-reference equivalence corpus are byte-unaffected and the
redaction guarantee keeps its meaning. The current reader already tolerates unknown keys
(`talaria/recorder/reader.py` selects each record's known fields and never rejects extras); U8
teaches it version 2 and the `profile` field, and `docs/formats/frame-log.md` records the
version-2 contract plus its unknown-key tolerance statement (U2's file scope). One log means the
interleaving is the native arrival order: replay determinism (AE8) needs no merge rule at all.
Rejected: one log per connection plus a run manifest — it invents a cross-log merge rule exactly
where determinism is the requirement. Rejected: keeping version 1 for multi-connection logs — the
first draft's choice, reversed on the panel's misread-hazard finding.

**KTD7 — The needs-you summary is a dedicated always-mounted one-row `NeedsYouBar`, opened by a
`/needs` local command.** A new widget beside HelpBar: `height: 1` fixed, composed at first
mount, never unmounted, explicit empty state (`needs-you: none`), ellipsis-not-wrap (the
composer-notice technique); count, oldest age, and oldest session title all through
`literal_text`; the source and age remain visible before the variable-width title at 80 columns.
The screen-row pins move once more, exactly as they moved for HelpBar
(`tests/transport/test_bridges.py:49` — "17, not 18: A4 added a one-row help footer"). The
drill-down opens via `/needs`, a new local command listed in the palette, checked against
`commands.catalog` for shadowing (fallback name `/needs-you`) the way `/sessions` established; no
new global key chord — the session switcher itself is command-only, and A4 retired the
function-key route. Rejected: a right-aligned HelpBar segment — this plan's first draft, reversed
on arithmetic: the A4 footer's curated binding text already fills roughly seventy of eighty
columns, so a summary segment forces cutting bindings A4 deliberately kept, and ellipsizing the
bindings defeats the row's purpose. Rejected: StatusRegion or PromptRegion — auto-height, the
proven reflow offenders.

**KTD8 — The confirm fires for live sessions Talaria does not drive; historical resumes are
exempt; a confirmed attach hydrates what the reply carries.** Talaria tracks ownership
(`we_drive` / `not_ours`) as its own bookkeeping: the sessions this run created, resumed, or
activated per connection. Activating — or live-resuming — a `not_ours` session in `active_list`
requires the OP2 confirmation, which states both possibilities ("this session may be attached to
another client; focusing it here detaches that client"), because rows carry no transport-identity
field and a detached orphan is indistinguishable from a driven session. For an `unobserved` queue
item the same dialog adds the kind warning: the waiting kind is unknown and may not be
recoverable after attach — the operator confirms a possibly-unresolvable steal knowingly, and the
non-recovery outcome latches visibly per KTD2's amended resolution rule. A historical session (in
`session.list`, not in `active_list`) resumes with no dialog: nothing live is stolen, and
always-confirming would train the operator to click through the one dialog that matters. After a
confirmed attach, the reply's `pending_approval` / `pending_clarify` hydrate cards (verified at
`server.py:8708`); a pending sudo or secret announced only to the displaced transport fails
visibly on the row and is never invented. U1's live leg observes the steal and the hydration
end-to-end; if a reply field ever distinguishes orphan from driven, the confirm copy sharpens and
the mechanism stands.

**KTD9 — Approvals answer inline through the one answer-path function; presentation stays
head-of-queue; an observed `request_id` is sent.** OP3 fixes the split. The inline answer and the
card's answer converge on the same function v0.2 established ("one function decides what an
answered prompt may claim"). Presentation keeps R18's head-of-queue rule — a second approval in
one session renders waiting-not-answerable exactly as the card path already does — and the answer
carries the head's `request_id` whenever one was observed (from an `approval.pending` row or a
request event; U1 records which carry it), because the gateway removes queue heads on timeout and
interrupt without emitting anything (`talaria/domain/state.py:959` documents the hazard; the
running revision synthesizes ids that aim exactly). With no observed id, the shipped
uncorrelated-approval refusal and deny-all fallback apply unchanged — correct on both revisions.
(Amended 2026-08-17 per R18's panel amendment; this reverses the first draft's unsent-always
choice.) Two mechanics ride with
this: the queue reuses the existing named-session guard in
`respond_live(..., session_id=...)` / `respond_to_prompt(..., session_id=...)` — the queue answers
a session it names and the guard verifies that ownership; and the in-flight `answering` set becomes
fleet-scoped, keyed
`(profile, session, request key)`, so a background inline approval occupies the switch-refusal
window exactly as a focused answer does (R6), and a late outcome applies to its queue item and
registry row — to the focused transcript only if that session is focused. Revisit when the probed
baseline floor guarantees `request_id` semantics everywhere Talaria runs.

**KTD10 — Lifecycle vocabulary: gateway words verbatim, Talaria classes on top.** Rows carry the
gateway's own `waiting | starting | working | idle` untranslated (re-encoding, not invention), plus
Talaria-derived row states: `streaming` for a driven session with an open turn, `reclaimed(reason)`
from the broadcast, `disconnected` when the row's connection is down. Orthogonal to all of it, the
three R24 display classes — live, stale-since, never-observed — qualify every value; a seeded row
whose status no poll has confirmed renders never-observed, not idle.

**KTD11 — `approval.pending` fires only at rows already reporting `waiting` or `working`.** The
build-warming side effect makes it unusable as a blind probe (the read-only discipline's letter would
be kept and its spirit violated). Presence-probing the *method* at startup uses the
parameter-invalid distinction (R11): a bare call answering "session required" proves presence without
naming a session.

**Amended by operator ruling, 2026-08-17, following U1's evidence.** The trigger fires at rows
reporting **waiting OR working**, not waiting alone as this text originally read. The rationale, as
the ruling states it:

> KTD11's gate exists to avoid warming lazy agent builds. U1 proved a pending approval does not
> surface as `waiting`, because approvals ride tools.approval's registry rather than the `_pending`
> prompt registry that feeds status — and a `working` row's agent is live by construction, because it
> is mid-turn. So firing approval.pending against waiting-or-working rows preserves the exact safety
> property — `idle`, `starting` and lazy rows stay excluded and are never polled — while keeping R14
> true for foreign approvals. The cost is bounded: only active rows, at the existing coalesce and
> backstop cadence.

The safety property stays checkable by pointing at code: `idle` excluded, `starting` excluded,
`working` included, `waiting` included, and no path firing it at a row of unknown status. In the
implementation that is `APPROVAL_DETAIL_TRIGGER_STATUSES` and `approval_detail_due()` in
`talaria/domain/queue.py`, with `approval_detail_targets()` in `talaria/domain/state.py` as the only
place the fleet decides to make the call.

**KTD12 — Ages ride the frame clock; polled values carry an observation floor.** Every age derives
from the clock domain that stamps frames (R20), so replay reproduces ages exactly. A wait age known
only from a poll (a foreign approval first seen mid-wait) renders as "waiting ≥ observed span"
until an authoritative start stamp exists — honesty over a fabricated start time, the same rule as
never-observed (R24). U1 records which start stamps `approval.pending` rows actually carry.

**KTD13 — The v1 status-line contract does not change meaning.** The external status payload
(`docs/formats/status-line.md`) keeps `pending_prompts` as the focused session's count
(`talaria/domain/projection.py:586`); the install-wide truth lives on the needs-you bar. Folding
the fleet count into a frozen field would silently change what every existing consumer reads —
rejected for exactly that reason. A fleet-scoped status field, if ever wanted, is a deliberate
versioned addition, not a meaning drift.

## Implementation Units

Dependency order. U1 gates everything; U2–U3 are the trunk; U4 follows U3; U5 can branch from U2;
U6 follows U4 because both change `talaria/domain/state.py`; U7 and U8 follow their named inputs;
U9 closes. The execution spec interleaves an adversarial-review unit (CR1–CR8) behind each
implementation unit as a scheduling gate; the spec graph is authoritative for order. Every review
unit returns a machine-readable verdict (`clean`, or `blocked` with its surviving findings), and
the /work driver halts on any non-clean verdict — remediating and re-reviewing before any
dependent unit starts. A review return is never treated as completion by mere existence.

### U1. The live verification and the new pinned read

**Goal:** Turn the grounding evidence into the recorded, operator-witnessed protocol baseline the
release builds on — and close the four questions only a live drive answers.

**Covers:** PC1 (recorded evidence for clauses a–e), KTD4's pin, KTD8's and KTD12's open details.

**Mechanism:** Preflight first, halt without it: the operator supplies the disposable test
workspace's identity and a throwaway-session inventory before the first mutating call — no
mutation happens against an unnamed workspace. Then, against the running install:
two concurrent connections to the configured gateway verify scoped routing empirically (a session
created on connection A produces nothing on connection B); a driven session raising each prompt
kind verifies `active_list` status transitions, `approval.pending` row shape and start stamps,
cross-session `approval.respond`, whether `approval.request` *events* now carry the synthesized
`request_id` (the pending-list snapshot does), and the attach handover end-to-end: the source says
the activate reply hydrates `pending_approval` and `pending_clarify` and nothing else — the live
leg confirms the steal, the hydration, and the visible failure of a non-hydrated sudo/secret. A second profile's gateway,
if paired on this machine, runs the cross-profile leg; otherwise that leg lands in U9's acceptance
run and says so. Everything lands in a dated evidence document plus updates to the protocol-surface,
reconciliation-rules, and Hermes terminal UI feature-inventory analyses. The affected inventory
rows receive explicit keep/change/drop verdicts against the running revision before implementation
begins (RR-28's drift is recorded with the new optional `request_id`).

**Files:** `docs/analysis/2026-08-XX-v0-4-topology-verification.md` (new),
`docs/analysis/hermes-gateway-protocol-surface.md`,
`docs/analysis/2026-08-02-hermes-reconciliation-rules.md`,
`docs/analysis/2026-08-02-hermes-tui-feature-inventory.md`.

**Test expectation:** none — evidence documents, no product code. The recorded captures become
U2/U3/U6 fixtures.

**Depends on:** nothing.

### U2. The connection set: N concurrent gateways, per-profile credentials, per-connection recording

**Goal:** Talaria dials every configured profile endpoint concurrently, each connection carrying
its own credential, epoch, reconnect loop, correlator, and probe state into one shared frame log.

**Covers:** R8, R22 (recorder boundary per connection), OP1, KTD1, KTD5, KTD6.

**Mechanism:** A `ConnectionSet` owns one `LiveSource` per inventory entry (config
`[profiles.endpoints]` plus the credential-file default); every frame is tagged with its connection
identity as it leaves the source, so the domain router (U3) never guesses origin. The credential
file gains `[profiles.<name>]` entries per KTD5; `refresh-credential --profile` acquires them. The
recorder stamps the optional per-frame `profile` key and the header `connections` list per KTD6
(writer half; U8 owns the reader and gate); a single-connection run emits neither. Two configured
endpoints that parse to the same URL share one connection only when their resolved tokens are
byte-identical; a token mismatch refuses loud and names both profiles (KTD5). `/profiles` stops
drop-switching:
selecting a profile *ensures* its connection is up and makes it the home for the next create or
resume — `switch_to_endpoint` stays as a lower-level primitive, not the fleet path — and a
profile with an endpoint but no credential renders `credential_unavailable` by name while every
other connection stays up. Per-connection failure isolation: one gateway down marks that
connection's rows stale (F4) and never stalls the others.

**Files:** `talaria/transport/source.py`, `talaria/transport/credentials.py`,
`talaria/transport/refresh.py`, `talaria/transport/connection_set.py` (new), `talaria/config.py`,
`talaria/recorder/framelog.py` (profile tagging), `talaria/cli.py` (refresh-credential flag),
`docs/formats/frame-log.md` (versioned multi-connection contract).

**Test scenarios:** `tests/transport/test_connection_set.py` (new) — two scripted gateways, one
dies: the other's traffic keeps flowing and the dead one's epoch bookkeeping stays isolated;
credential resolution refuses a missing profile entry loudly and never borrows another profile's
token; a mode-0644 credential file is refused exactly as it is today; selecting a
second profile leaves the first connection's socket up (asserted by a subsequent call on it
succeeding without a redial); two configured rows parsing to one URL produce one connection.
`tests/recorder/test_profile_tagging.py` (new) — a two-connection run writes one log whose frames
carry `profile` and whose header lists `connections`; a single-connection run emits neither, so
`tests/recorder/test_equivalence.py` continues to pass untouched; a v0.1-format log with no
`profile` key loads as one connection. A synthetic credential canary passing through each
connection is absent from the resulting log.

**Depends on:** U1.

### U3. The registry: root object, routing, seeding, retirement

**Goal:** Every session of every connected gateway is a bounded row in domain state; events route
to rows instead of being discarded; absence of observation is named, never faked.

**Covers:** R1, R3 (row fields), R4, R5, R20, R24, R25, PC2, KTD2, KTD3,
KTD10, KTD12.

**Mechanism:** `FleetState` per KTD3, rows keyed `(profile, durable_id)` with runtime-id aliases.
Seeding: `session.list` at attach for identity — list-only rows enter as **never-observed**, not
idle — and `session.active_list` for liveness; the poll loop per KTD2. Row content per PC2:
lifecycle (KTD10 vocabulary), ownership (`we_drive`/`not_ours`, KTD8), waiting-kind when known
(`unobserved` when only the flattened `waiting` is), last-event source and age (KTD12), a bounded
`last_notice` line (expiry, terminal-read failure), title, message count, model — a row is a
fixed-size summary, never a transcript. Memory bound: 256 rows per connection (the listing default
is 200 plus headroom for live-only sessions), applying only to unprotected rows — the focused row,
rows holding queue items, and rows with in-flight answers are always retained; when unprotected
rows exceed the remaining budget the oldest-by-last-observation are evicted and the roster renders
a visible truncation note ("N older sessions not shown"), never a silent drop, and protected rows
may exceed the cap outright. Retirement: a row absent from **both** a successful
`session.list` and `session.active_list` on the same epoch is dropped only if it is not focused,
holds no queue item, and has no in-flight answer; `session.reclaimed` (reason recorded) and
connection loss mark stale-since — never silent removal while a queue item references the row;
live-only rows (in `active_list`, not the historical listing) are kept. Routing per KTD3: the focused engine is fed
exactly as today; foreign identified events update rows; unknown-session events create rows (R4);
identity-less events surface on the connection-scoped channel and are counted (R25). The
`/sessions` picker rows read registry summaries (R3's second half lands in U4 where the picker is
touched).

**Files:** `talaria/domain/state.py` (FleetState, router), `talaria/domain/registry.py` (new),
`talaria/domain/session_list.py` (active-list row decode beside the existing listing decode),
`talaria/domain/decode.py` (reclaimed/changed wiring).

**Test scenarios:** `tests/domain/test_registry.py` (new) — AE1 verbatim: a background session's
event (a known kind and an unknown kind) updates its row, leaves the focused transcript unchanged,
and the test fails against the current discard-and-count behavior; an identity-less event surfaces
on the connection channel, is counted, creates no row; a seeded-but-never-polled row renders
never-observed, not idle (AE4's second half); a reclaimed row shows stale-since with the reap
reason; a dropped connection marks every row it fed stale-since with age, never frozen-fresh
(AE4's first half at row level); ages are computed from frame stamps and reproduce exactly under
re-application (AE8's domain half). `tests/domain/test_registry_bounds.py` (new) — 200 listed sessions produce bounded
row memory; a row never accumulates event history; at the cap, at the cap plus one, and with more
protected rows than the cap: no focused, queued, or in-flight row is ever evicted, eviction is
oldest-first among unprotected rows only, and the truncation note renders whenever anything was
evicted.

**Depends on:** U2.

### U4. Focus movement: churn repair, confirm-before-steal, the registry-backed picker

**Goal:** Focus movement is correct under churn, honest about taking another client's session, and
the `/sessions` picker shows what each session is doing.

**Covers:** R2, R3 (picker rows), R6, PC6, OP2, KTD8.

**Mechanism:** The focus-churn repairs already shipped — `focus_session` keeps `prompts`,
`flushed_prompt_ids`, and `approvals_seen` across a switch and resets `withdrawn_approvals`,
pinned by `tests/domain/test_prompt_registry.py` — so this unit keeps those pins green under the
registry rather than re-fixing them, and corrects the two stale register entries when it lands.
What it adds: the switch-refusal rule reported through every path — queue navigation and the
picker both surface the refusal reason, with the `answering` set fleet-scoped per KTD9 so a
background inline answer also refuses a switch; the OP2 confirmation per KTD8 (live `not_ours`
rows only — historical resumes stay dialog-free); the attach handover rendering the reply's
hydrated approval/clarify cards and failing a non-hydrated sudo/secret visibly on the row. The
picker's rows render registry summaries: lifecycle, waiting-on, source, and age — replacing the
bare listing rows. Every gateway-derived label is defanged and rendered as literal text; sensitive
response values never enter a picker row.

**Files:** `talaria/domain/state.py` (`focus_session` repairs), `talaria/ui/app.py` (confirm flow,
refusal reporting, picker dispatch), `talaria/ui/picker.py` (registry-backed session rows),
`docs/engineering-journal/QUEUED.md` (the two stale register entries this unit corrects).

**Test scenarios:** `tests/ui/test_sessions.py` (extend) — AE6 verbatim against both defect
mechanisms; a switch mid-answer is refused with the reason on screen; activating a foreign row
shows the confirmation and a decline changes nothing (fire-and-observe: no optimistic focus move).
`tests/ui/test_focus_churn.py` (new) — rapid switch across three sessions with outstanding prompts
in each; every prompt answerable when its session is refocused. At 80 columns, each picker row keeps
source and age visible while a markup, control-sequence, and credential canary remains inert and
redacted in the rendered screen. A scripted activate reply carrying `pending_approval` and
`pending_clarify` renders both cards on landing; the same reply without them latches the visible
resolved-failed on the row for a known-waiting sudo/secret — the headless twin of U1's live
hydration leg.

**Depends on:** U3.

### U5. Seam probes: named presence, absence, and degradation per connection

**Goal:** Startup names what each connection's install can and cannot do, absence renders as a
named absence with its disabled feature, and the probe story stays live.

**Covers:** R9, R10, R11, R12, R20, R24, PC3, KTD4, KTD11 (presence probing).

**Mechanism:** Per connection, the probe set is: the six pinned read-only baseline methods;
`session.active_list` (bare call); `approval.pending` presence via the parameter-invalid
distinction (R11 — "session required" proves presence, method-not-found proves absence, and any
parameterized failure is re-asked bare before absence may be claimed, closing the recorded
`absent_capability` misdiagnosis); the HTTP runner seam probed by a bare `GET /api/health`
(present at the running revision, `hermes_cli/web_server.py:3280`) on the origin the admin client
already uses, naming `http-runner` present or absent with the admin catalogue as the disabled
feature. The `kanban-dispatcher` seam has no route to probe at the running revision, so it renders
**never-observed** — "board queue source off", never "zero cards" — and no probe URL is invented
for it (inventing one is the `absent_capability` error class again). Classification: present,
absent, incompatible, degraded, parameter-invalid, plus the never-observed display class. Cadence: at attach, on every reconnect, and a
low-frequency revalidation (5 minutes) — never per-render; a mid-session degradation updates the
named surface with source and age (R12). Probe errors expose bounded classifications rather than
raw response bodies; gateway-derived text is defanged and rendered literally. Replay: recorded probe replies replay; a seam the
recording never observed renders never-observed (R24); no socket opens.

**Files:** `talaria/domain/compat.py`, `talaria/transport/compat_check.py`,
`talaria/transport/admin.py`, `talaria/ui/status_region.py` (seam lines).

**Test scenarios:** `tests/transport/test_seam_probes.py` (new) — AE3 verbatim: an absent
`session.active_list` names the disabled roster; a parameterized failure is re-asked bare before
absence is claimed; a seam that degrades mid-session updates its line with source and age; markup,
control-sequence, and credential canaries remain inert and redacted in the rendered seam line.
`tests/domain/test_probe_replay.py` (new) — probes under replay: recorded replies reproduce; an
unobserved seam is never-observed; no dial occurs.

**Depends on:** U2 (runs per connection); parallel to U3 otherwise.

### U6. The needs-you queue: typed items, two feeds, fire-and-observe

**Goal:** Every outstanding human-facing blocking prompt from every registry session is one typed
item in one flat wait-age-ordered queue, fed by events where Talaria drives and by polls where it
does not, and answers never render optimistically.

**Covers:** R13, R14, R15, R18, R19 (domain half), R20, R21, PC4 (item semantics), KTD2, KTD9,
KTD11, KTD12, KTD13 (the `pending_prompts` meaning pin).

**Mechanism:** One item type: reference `(profile, session_id, request_key)`, source, kind, prompt
text, allowed answers, age — kind is an open set (the running gateway already has three kinds the
pin never named). Feed A (driven sessions): a new projection over the existing prompt registry
exposing `opened_at`/`seq` (they exist and are dropped today), all kinds. Feed B (foreign
sessions): rows reporting `waiting` become items — approvals with full detail via KTD11-gated
`approval.pending`, everything else as the `unobserved` kind. One item identity across both
feeds so a session Talaria attaches mid-wait never duplicates its item. Ordering: oldest wait first
(R15), age per KTD12. Answering: approvals head-of-queue per KTD9; a second same-session approval
renders waiting-not-answerable (AE2); in-flight renders requested-with-age; resolution only on
gateway confirmation or expiry; ambiguous outcomes settle and latch (the recorded decision). A
background terminal-read fails visibly on its session's row (its bounded `last_notice`) and, when
focused, the transcript — and the failure **settles**: the bridge's unavailable path today leaves
the prompt registered (the recorded unavailable-projection defect), and this unit latches it
resolved-failed instead. It never enters the queue (R14 — the queue holds only resolvable items).
Cross-session answering reuses the existing named-session guard per KTD9 — the caller names the
session and the guard verifies the match; it is never bypassed.

**Files:** `talaria/domain/queue.py` (new), `talaria/domain/projection.py` (new projection only),
`talaria/domain/models.py` (item type), `talaria/domain/state.py` (feed wiring in `FleetState`),
`docs/formats/status-line.md` (the focused-scope qualifier on `pending_prompts` — a clarification,
not a meaning change).

**Test scenarios:** `tests/domain/test_needs_you_queue.py` (new) — AE2 verbatim (head-of-queue,
second approval waits, "gateway not waiting" settles and latches); AE5 verbatim (no confirming
event → requested-with-age indefinitely, late confirmation resolves exactly once); a driven and a
polled sighting of the same approval produce one item; a foreign non-approval waiting session
yields an `unobserved` item; expiry clears item and updates count in one reduction (R19's domain
half); wait-age order is stable under equal ages (seq tiebreak); a poll-first-seen approval
renders "waiting ≥ observed span" until an authoritative start stamp exists and never a fabricated
start time (KTD12); the status payload's `pending_prompts` stays the focused session's count with
the fleet queue non-empty — the KTD13 pin, extending `tests/domain/test_projection.py`.

**Depends on:** U4.

### U7. The needs-you surface: reserved summary, drill-down, inline approvals

**Goal:** Talaria's live launch becomes U2's production consumer, and the queue is discoverable at a
glance in space that cannot reflow anything, drillable per shipped picker conventions, and every kind
resolvable keyboard-only.

**FIRST NAMED DELIVERABLE — the production composition root (operator ruling, 2026-08-18).** Ahead of
any surface work. `build_live_app` (`talaria/cli.py`) assembles the `ConnectionSet`
(`talaria/transport/connection_set.py`) with registry-rooted routing, so the running application
dials every configured profile rather than one, plus an app-assembly test that fails if the entry
point ever reverts to a single `LiveSource`.

*Why it is U7's and not U8's or U9's.* U2 built the connection set and its two-gateway tests; nothing
ever assembled it, so `TalariaApp.connections` was permanently `None` and every multi-connection
branch was dead. Without this wiring U7's own needs-you surface would be fleet-capable only in
fixtures — the defect class U6's round-ten review found at function scale, repeated at application
scale. It is a prerequisite of U7's production claim, not a late detail before acceptance. **U9's
two-profile live acceptance leg depends on it**, and would otherwise be the first thing to discover
it.

*Four conditions, all operator-set.* (1) This amendment names the `/profiles` behaviour flip
explicitly: populating `connections` changes `/profiles` from drop-switching to ensure-beside, which
is decided design, not a new choice — see U2's mechanism above ("`/profiles` stops drop-switching")
and the `ConnectionSet.ensure` entry in `docs/engineering-journal/DECISIONS.md`. (2) The picker tests
asserting the drop-switch behaviour are rewritten to assert the new meaning and say why, never merely
repointed. (3) The `# pragma: no cover` on `_ensure_profile` (`talaria/ui/app.py`) comes off with
real coverage in the same commit that makes it live. (4) U6's seam-board carry-forward fires here:
production seam probing goes per-connection, and the single-board reading is retired on evidence —
`FleetState.seam_boards` is already a per-connection mapping, but `app.seams` reads only the focused
profile's board and `_reprobe_seams` runs from the single-connection callback.

*How the four resolved (2026-08-18).* (1) Done — this section names the flip and both authorities.
(2) Already satisfied by U2, which wrote its picker tests for both shapes before either could be
reached; the application-level pin on the routing itself is
`tests/ui/test_picker.py::test_selecting_a_profile_ensures_its_connection_and_never_switches`, which
drives the real `TalariaApp` through `/profiles 1` and asserts `connections.ensure` was called and the
switcher untouched. (3) The pragma stays, on the operator's confirmation: it sits on a defensive
`if connections is None` branch *inside* `_ensure_profile` that the sole caller already makes
unreachable, not on the function, which the picker tests cover. (4) Done, and it needed more than the
literal ask — `app.seams` reading the focused board was correct and stayed; what was single was the
**probing**. `TalariaApp.sweep_connection` now probes *and* sweeps one named connection into its own
board and its own registry rows, on that connection's `connected` transition and on the revalidation
timer. The pairing is not tidiness: a probe alone deletes the queue's "capabilities not probed"
sentence while enumerating nothing. See DECISIONS.md, "A background connection is probed and swept in
one round".

**REPORTED OPEN, not closed here — the KTD2 poll loop has no production caller.**
`next_poll_due_at` (`talaria/domain/registry.py:265`) carries the whole cadence — a 2-second coalesce
after a `sessions.changed` hint, a 30-second backstop — and nothing in `talaria/` calls it. Nor does
anything consume `channel.hint_at`, which `route_frame` records on every `sessions.changed`
(`talaria/domain/state.py:3224`) and only `apply_active_list` clears. So today a gateway announcing
that its session list changed is heard and not acted on, and the only roster folds in production are
this unit's per-connection sweeps plus the focused fold behind `/sessions`. This is a standing traffic
commitment against every configured gateway and belongs to a unit that names it; it is surfaced for
assignment rather than absorbed into the composition-root commit.

**Covers:** R16, R17, R18 (surface half), R19, R21, R22, R23, PC4, OP3, KTD7, KTD9, AE7, AE9,
AE11.

**Mechanism:** The dedicated `NeedsYouBar` per KTD7: one fixed row, composed at first mount,
never unmounted, empty state `needs-you: none`, ellipsis-not-wrap, count plus oldest item's source,
age, and session; source and age remain visible before the variable-width session title at 80
columns. The screen-row pins that moved once for HelpBar move once more, with a comment naming
this bar. `/needs` opens the drill-down — a local command checked against `commands.catalog` for
shadowing (fallback `/needs-you`), listed in the palette, no new global chord (the session
switcher's own precedent); the modal follows picker conventions — arrows move, typing filters,
`escape` leaves, composer draft survives. `enter` navigates to the item's session (through U4's
confirm when the session is live and not ours), landing with `focus_first_unanswered` so the caret
reaches the card; on head-approval rows, explicit approve/decline keys answer inline through the
single answer-path function with an explicit deny — never an empty choice — and the card path and
the queue path converge on it (KTD9).
Every rendered string from the gateway crosses `defang` + `literal_text` (R23); no sensitive
respond value reaches any row (R22). Expiry is visible twice in the same render boundary (R19).

**Files:** `talaria/cli.py` (the composition root — `build_live_app` assembles the `ConnectionSet`),
`talaria/ui/needs_you.py` (new: the bar and the drill-down source), `talaria/ui/app.py` (the tagged
frame pump, compose wiring, `/needs` routing), `talaria/domain/commands.py` (the local command and
its catalogue shadowing), `talaria/ui/prompts.py` (answer-path reuse only).

**Test scenarios:** `tests/ui/test_needs_you.py` (new) — AE9 verbatim via the geometry-invariance
pattern (`tests/ui/test_status_region.py:110` is the template): empty → one → many → empty, no
widget's height moves, transcript never reflows; AE11 verbatim: headless keyboard-only run
resolves every kind (approval inline; clarify/secret/sudo/`unobserved` by navigation with the
caret landing on the card's control); AE7 verbatim: synthetic credential-bearing and
markup/ANSI/HTML-bearing background traffic — nothing styles, executes, or leaks, withheld values
leave markers; an inline answer renders requested-with-age and never optimistically clears (R21); an inline
decline sends the explicit deny choice, never an empty string; `/needs` resolves as a local
command ahead of the gateway catalogue (the `/sessions` shadowing pattern), and escape from the
drill-down restores the composer caret with the draft intact.

**Depends on:** U4 (confirm flow), U6 (queue domain).

### U8. Fleet recording and replay: the determinism gate, extended

**Goal:** A multi-connection recording replays into the same registry, queue, and focused
projection every time, ages included, with no socket.

**Covers:** R7, R20, PC5, AE8, KTD6, KTD12.

**Mechanism:** Replay reads the per-frame `profile` tags (a log with none is one connection —
KTD6), preserving native arrival order with no merge rule; the focus derivation is the recorded
landing replies when present, else first-session-named (today's adoption rule), and the gate
asserts the derivation itself, not only the end state. The existing determinism checks are
extended with registry, queue, derived-focus, and rendered-age checkpoints — same checkpoints,
twice, byte-identical — rather than a parallel gate. Mutation controls inert per v0.1 R40.

**Files:** `talaria/replay/source.py` (profile-tag reader), `talaria/replay/gate.py`
(registry and queue checkpoints added to the existing determinism gate).

**Test scenarios:** `tests/replay/test_fleet_replay.py` (new) — AE8 verbatim on a U1-captured
two-session recording: two runs, identical registry/queue/focus/age state at every checkpoint;
recordings in the v0.1–v0.3 format (no profile tags) still replay unchanged.

**Depends on:** U2 (format), U3 (registry), U6 (queue).

**Operator ruling, 2026-08-18 — U8 owns one open question from U6's review.** Determine whether a
**keyless approval** can arrive from any supported recording format: an `approval.request` carrying
no `request_id`. Cite what you find, either way, against the formats this unit already has to replay
(v0.1–v0.3 without profile tags, and the current one).

Why it matters beyond replay: U6's unplaceable fold refuses an approval only when Talaria holds no
gateway id for it, and the case for deleting that fold outright rests on the claim that blind items
cannot arise. That claim is currently revision-specific — the live gateway mints a `request_id` for
every entry — and nobody has checked the replay path or older logs, which is where a keyless approval
would still reach the domain. Until U8 answers it, the fold ships as insurance with a stated trigger.
If U8 proves blind items impossible across every supported input, the fold-deletion clause in
`docs/engineering-journal/DECISIONS.md` (2026-08-18, "The unplaceable fold refuses only what would be
answered blind") becomes actionable at the alias-pinning revisit. If U8 finds one, the fold is
load-bearing and the clause is withdrawn.

### U9. The live acceptance run

**Goal:** The release's claims are witnessed on the real install before they are claims.

**Covers:** AE10 end-to-end (F2 live), plus any U1 leg deferred for want of a second paired
profile.

**Mechanism:** Operator-driven, recorded as release evidence: sessions from at least two profiles
(a second configured and reachable profile endpoint plus its paired credential are named
prerequisites, with pairing sized in KTD5's `refresh-credential --profile`), a real background
prompt discovered from the summary, opened in
the drill-down, answered — inline for an approval, by confirmed navigation for another kind — and
the confirmed clear observed everywhere in one boundary; plus the steal leg — attempt to activate
a session the native TUI is driving, see the dialog, cancel, then confirm and watch the native
client lose the session. v0.3 shipped with no unit gated on a live drive; this release does not
repeat that. Safety envelope: throwaway sessions only, never the operator's working sessions;
approval legs use a canary command that grants nothing; sudo/secret legs navigate and decline — no
live secret is ever typed; recordings are redaction-checked before they are cited; a failing leg
stops the run rather than improvising; the run closes what it opened. The automated pipeline's
terminal state is explicitly operator-attention: checklist and skeleton existing means the
workflow *stops and waits*, and the release gate is the separately completed, operator-verified
evidence document with a pass verdict — workflow completion never reads as release-ready.

**Files:** `docs/plans/2026-08-XX-v0-4-live-acceptance-results.md` (new, evidence).

**Test expectation:** none — operator-driven evidence document; the automated equivalents live in
U4/U6/U7's suites.

**Depends on:** the reviewed surfaces — in the execution spec's graph, CR4, CR5, CR7, and CR8 (U8's
replay gate lands before acceptance staging).

## Risk Analysis & Mitigation

- **The attach handover's residue.** The activate reply hydrates pending approvals and
  clarifications only (verified at source); a sudo or secret announced to the displaced transport
  is unrecoverable by design. Mitigation: KTD8 renders that as a visible failure on the row, never
  an invented card; U1's live leg confirms the whole handover; the queue never promised those
  kinds for foreign sessions in the first place (their pending state is not queryable).
- **Gateway drift between the U1 pin and execution.** The install auto-updates; `7095e23eb` may
  not be running when U2–U8 land. Mitigation: every new capability is probed (KTD4), so drift
  degrades to named absence, not breakage; U1's evidence document records the pin date so a drift
  is a diff, not an archaeology.
- **Poll pressure on large installs.** 200 listed sessions with the 2-second coalesce could make
  `active_list` polling feel like traffic. Mitigation: `active_list` snapshots in-memory state
  (cheap by construction at the source, verified in U1); the coalesce and 30-second backstop are
  KTD2 constants, tunable in one place; `approval.pending` fires only for `waiting` rows (KTD11).
- **The chrome-row cost (KTD7).** `NeedsYouBar` permanently spends one screen row, as HelpBar did
  in v0.3. Mitigation: the row-count pins move once with a named comment (the A4 precedent,
  `tests/transport/test_bridges.py:49`); the bar cannot wrap or grow with the count; AE9's
  geometry test runs at 80 columns.
- **Credential-file migration (KTD5).** A malformed multi-entry file must not lock the operator
  out silently. Honest boundary: the single-entry shape stays valid forever (it *is* the default
  profile), but one TOML document has one parse — a syntax error fails the whole file loud with
  the line named, exactly as today; per-profile isolation exists only for semantic validation
  after a successful parse (a bad table refuses that profile by name and leaves the rest usable).
- **Second-profile pairing never happens.** U9's two-profile leg depends on an operator act.
  Mitigation: named as a prerequisite in U9, tracked from U1; if genuinely unavailable at release
  time, the release notes state the single-gateway limit per the claims rule — the code path is
  still exercised by U2's two-scripted-gateway tests.

## Scope Boundaries

Everything the requirements document lists stays out (Kanban and pane-manager sources, the fleet
header and event log, spawn/steer verbs, attention budgeting, theming, Linux, multi-host, PyPI).
This plan adds, as deliberate deferrals recorded here:

- **`approval.respond` with `request_id`** — capability recorded, unsent (KTD9). Revisit when the
  probed baseline floor includes it everywhere Talaria runs.
- **The remote-mode `profile` parameter** — real, unused (KTD1). Revisit if a future release wants
  Talaria-spawned sessions on profiles without their own dialable endpoint.
- **The three new gateway prompt kinds** (`preview.read`, `window.read`, `mcp.setup`) get the
  kind-open item treatment, not per-kind affordances. Per-kind support is future work, and the
  queue's type admits it without restatement (R13).
- **Sessions of unconfigured profiles** — undialable, therefore out; named as an absence, not
  rendered as zero.

## Planning Closure

- **PC1 — closed by evidence, sized by OP1.** Topology: N connections per configured endpoint
  (KTD1); clauses a–e answered in Grounding Evidence and formalized by U1; credential consequence
  sized in KTD5/U2. The rejected branch and its rationale are recorded in KTD1.
- **PC2 — closed in U3/KTD10/KTD12** (row fields, vocabulary, memory bound, retirement,
  re-keying).
- **PC3 — closed in U5** (membership, classification, cadence, replay behavior).
- **PC4 — closed in U7/KTD7/KTD9/OP3** (segment, empty state, keybinding, drill-down, per-kind
  resolve).
- **PC5 — closed in U8** (gate assertions, focus derivation, age reproduction).
- **PC6 — closed in U4** (both recorded defects, refusal reporting).

## Sources

- `docs/brainstorms/2026-08-16-talaria-v0-4-fleet-turn-requirements.md` — the WHAT; R/F/AE/PC
  numbering used throughout.
- Operator rulings, 2026-08-16 (this session): OP1 multi-profile now; OP2 confirm-before-steal;
  OP3 inline approvals only.
- Hermes gateway source at revision `7095e23eb` (`~/.hermes/hermes-agent`, the running install):
  citations inline in Grounding Evidence; to be formalized as the pinned read by U1.
- Live probe, 2026-08-16: bare-connection listen (90s, silence), `session.list` live row shape,
  auth and envelope confirmation. Raw captures in session scratchpad; not repository artifacts.
- `docs/engineering-journal/DECISIONS.md` — the Q1 ruling; the answer-path, settle-and-latch, and
  respond-value decisions v0.2 recorded; the v0.3 B1 height-stability decision KTD7 builds on.
- `docs/engineering-journal/QUEUED.md` — the two `focus_session` defects (U4), the
  `absent_capability` misdiagnosis (U5), the unavailable-projection terminal-read defect (U6).
- Transport and UI survey memos, 2026-08-16 (in-session, read-only): all `talaria/` citations
  above verified against the working tree at the requirements merge.
- Independent comparison plan, 2026-08-17: a second plan was authored from the same requirements,
  evidence pack, and rulings by an isolated Grok 4.6 session that never saw this document. The
  comparison adopted (after re-verifying every claim at source): the activate-reply hydration
  fact, the historical-versus-live confirm scoping, the durable-id registry key, the fleet-scoped
  answering set and reuse of the named-session answer guard, the dual-listing retirement rule, the
  `/needs` command with catalogue shadowing, the dedicated `NeedsYouBar` (reversing the HelpBar
  segment), the single-log profile-tagged recording format (reversing the manifest), the
  kanban-never-observed and `GET /api/health` probe treatment, the status-line contract guard,
  the terminal-read settle fix, and the live-run safety envelope. Rejected from it: dropping the
  live-verification unit (the pinned-read discipline requires U1), and conditionally sending
  `request_id` on head approvals (two wire shapes for marginal benefit; recorded as the revisit
  path).
