---
date: 2026-08-17
topic: v0-4-topology-verification
scope: U1's operator-witnessed live protocol baseline for the v0.4 fleet turn — scoped routing, per-kind status transitions, the approval surface, and the attach handover, verified on the wire
status: recorded — this is the evidence the v0.4 plan's Grounding Evidence pointed at, now driven live
source: Hermes Agent 7095e23eb (the running install's checkout) and 91a545ab1 (the revision the serving process actually executes — see "Two current revisions")
---

# v0.4 topology verification — the live leg of U1

The [v0.4 fleet-turn plan](../plans/2026-08-16-talaria-v0-4-fleet-turn-plan.md) grounded its design
in a source read of the running install's checkout and left five questions that only a live drive
answers (unit U1; PC1 clauses a–e; KTD8's and KTD12's open details). This document records that
drive. It took **two probe runs**, four connections and two throwaway sessions in total. The first
run held two concurrent authenticated websocket connections — called **A** and **B** — against the
default configured gateway and drove one throwaway session through the prompt kinds; every claim
below about routing, status transitions, steal and hydration comes from that run. The second run
held two further connections — **C** and **D** — against the same gateway with a second throwaway
session, and is the source of every claim about the approval surface (method presence, the
`request_id` parameter's acceptance) and of the one `starting` observation. Both sessions were
created by the probe runs themselves with `close_on_disconnect` set and closed by their run. Every
inbound and outbound frame was captured with timestamps; captures live in the session scratchpad
(machine-local, not repository artifacts) and were checked for the credential before anything here
was cited. No operator session was read, activated, or answered — the serving process held zero
other live sessions for the whole run.

## The headline finding: two "current revisions" exist at once

**The checkout at `~/.hermes/hermes-agent` is `7095e23eb`. The process serving the websocket
Talaria dials is not running it.** That process started seven days before the checkout advanced to
`7095e23eb` (a fast-forward merge made 2026-08-16), and a Python process executes the code it
imported at start. The revision it does execute is `91a545ab1` — the checkout's state when the
process started. **All four profile dashboard processes — the ones serving `/api/ws`, which is the
surface Talaria dials — started within one second of each other inside that window**, so the whole
websocket surface of this machine is `91a545ab1` until those processes restart. The claim is scoped
to them deliberately: four further `gateway run` processes are also up, started at three other
points in the checkout's history (one of them after the advance to `7095e23eb`), but they answer 404
on `/api/ws` and are not a surface Talaria dials.

This is not a technicality; the two revisions differ on exactly the surface v0.4 builds on:

| Surface | `91a545ab1` (the live wire today) | `7095e23eb` (the checkout / pinned read) |
| --- | --- | --- |
| registered RPC methods | 135 | 154 |
| `session.active_list` | present (verified live) | present |
| `approval.pending` | **absent** — `-32601 unknown method` (verified live) | present (`tui_gateway/methods_prompt.py:1448`) |
| `approval.received` (ack) | absent | present (`methods_prompt.py:1461`) |
| `approval.respond` | present; accepts `request_id` without error (verified live); **cannot read it** — zero occurrences of `request_id` in `tools/approval.py` at this revision, so FIFO head or `all` only (source-derived) | present; optional `request_id` resolves the named entry (`tools/approval.py:2655-2662`) |
| approval `request_id` synthesis | none — `_ApprovalEntry` carries no id | every entry gets a `uuid4` id at construction (`tools/approval.py:2596`) |
| `session.activate` reply hydration | **none** — no `pending_approval`/`pending_clarify` keys exist in the payload builder (verified live and at source) | hydrates `pending_approval` + `pending_clarify` (`tui_gateway/server.py:8708-8711`) |
| `mcp.setup.respond` bridge | absent | present (`methods_prompt.py:1429`) |
| blocking request bridges | 7 kinds | 8 kinds (+ `mcp.setup`) |

Consequences, in order of how much they bind v0.4:

1. **KTD4's probed-absence design is not hypothetical — it is this machine today.** A gateway with
   `session.active_list` but no `approval.pending` is what Talaria will actually dial until the
   serving processes restart. The probe distinction works cleanly on the wire: an absent method
   answers JSON-RPC `-32601` (`"unknown method: approval.pending"`), while a present method with a
   bad session answers `4001` (`"session not found"`). "Roster available, approval detail
   unavailable by name" is a real, reachable state, not a defensive fiction.
2. **KTD8's hydration will silently not happen on today's wire.** A confirmed attach on
   `91a545ab1` recovers no cards — the exact "attach that hydrates no card" path whose visible
   resolved-failed latch KTD2 specifies. The latch is therefore load-bearing on day one, and U9's
   acceptance run must exercise both hydration outcomes (present after a restart, absent before).
3. **RR-28's drift is real but not yet live.** The request-id half of the drift (synthesized ids,
   aimable respond) exists only at `7095e23eb`. On today's wire, `approval.respond` carrying a
   `request_id` is *accepted without error* — verified live, against an empty queue, so the reply
   was `{"resolved": 0}`. That it is silently *unaimed* is source-derived rather than measured:
   `tools/approval.py` at `91a545ab1` contains zero occurrences of `request_id`, so the parameter
   cannot be read and the FIFO head is the only reachable target. KTD9's rule (send an id only when
   one was observed) is safe precisely because an id can
   only be observed from a gateway that synthesizes them; but a recorded id replayed across a
   gateway downgrade would be silently misaimed, not refused. Recorded in the reconciliation-rules
   catalogue's drift section.
4. **The pinned read stays pinned to `7095e23eb`** — that is the code the next restart brings up,
   and the revision U2–U8 build against. Where this document says "verified live," the claim is
   about `91a545ab1` behavior; where it cites `server.py`/`methods_prompt.py` lines, it cites the
   `7095e23eb` checkout.
5. **A correction to the plan's KTD4: `session.active_list` is not a new method.** KTD4 names it
   alongside `approval.pending` as a method "the old pin (`7f4d15515`) does not have". The
   enumeration says otherwise: `session.active_list` is registered at `7f4d15515` too, the delta
   from that pin is exactly 24 methods added and none removed, and the roster method is not among
   them. Its status vocabulary is unchanged as well — `_session_live_status` is byte-identical at
   all three revisions. Only `approval.pending` is genuinely new. Nothing in the design breaks:
   probing an always-present method simply always succeeds, and the degraded "roster unavailable"
   branch stays as defence against gateways Talaria has not met. But the roster must **not** be
   gated behind a version check, and the plan's sentence should not be read as license to build one.

The generalizable rule went to the engineering journal: a checkout revision is evidence about a
running system only after the serving process's start time is checked against it.

## PC1 clause a — scoped routing, verified empirically on two connections

Setup: A and B connected and authenticated; A created the throwaway session; B held still.

- **Creation leaked nothing.** In the 8 seconds after `session.create` on A, B received zero
  session-scoped frames for the new session. B's only traffic was `sessions.changed` — broadcast,
  session-less, empty payload.
- **A full streamed turn leaked nothing.** During A's first driven turn (streaming deltas, tool
  events, completion), B again received zero frames carrying the session id; only
  `sessions.changed` broadcasts (observed coalescing at 2.02–2.05-second spacing, matching the
  server-side broadcast floor `_CHANGE_BROADCAST_FLOOR_S` at `server.py:3748`. The plan cites
  `server.py:3736`, which is the `_CHANGE_WATCHES` entry setting the 0.5-second *check* interval —
  a different constant; the 2-second number the plan quotes is the floor, at `:3748`).
- **The whole-run totals make it airtight.** Across the run, connection A received 406
  session-scoped events and B received 2,084 — and the split tracks transport ownership exactly.
  Before B first took the transport it had received **zero**. After each ownership flip the
  displaced connection's session-scoped event count stopped increasing — to zero further events,
  mid-turn, with no terminating notification of any kind.

A registry cannot be fed by ambient listening; `session.active_list` polling plus the broadcast
hints is the only feed. The requirements' question is settled at the wire, not just at source.

## PC1 clauses b/c — `session.active_list` status transitions, per prompt kind

Polled from a connection other than the one driving. **The poll stream was not uniform**, and the
table below depends on knowing where it was thin: the watcher on connection B polled 255 times but
carried a 367-second hole and a 30-second hole, and connection A polled only 6 times in the whole
run, its first poll landing *after* the clarify request had already blocked the turn. Each cell is
therefore marked for whether a poll actually backs it.

| Prompt kind | States a poll actually read | States inferred, not polled | Status while blocked |
| --- | --- | --- | --- |
| plain turn | `idle → working → idle` — all three polled | none | `working` |
| clarify | `waiting → working → idle` | the leading `working`: no poll fell between submit and block | **`waiting`** (polled) |
| sudo | `waiting → working → idle` | the leading `working`: same gap | **`waiting`** (polled) |
| secret | `waiting`, then held for the whole 240-second window with a second request still pending (no timeout was observed — see below) | the leading `working`: no poll fell in the 27-second window between submit and the two requests | **`waiting`** (polled) |
| approval | **not observable on this install** — see below | the whole row | `working` (source-derived, unverified live) |
| terminal.read / preview.read / window.read / mcp.setup | not induced (desktop-GUI tools; the throwaway agent had no reason to call them) | the whole row | `waiting` by construction — same `_block()` registry as clarify |

The load-bearing half of each row — **blocked implies `waiting`** — is genuinely polled in all three
induced kinds. The trailing `working → idle` tails are polled for clarify and sudo; the secret leg
has no tail at all, because its turn never resumed — every poll from the answer onward read
`waiting`, the second request still pending. Only the leading `working` is inferred, from the turn
being in flight when the prompt fired.

Notes that matter:

- **`starting` was observed once, and a routine poll caught it.** The second probe run polled
  `active_list` every 2 seconds. Its **first** poll, 6 milliseconds after the `session.create`
  reply, carried `"status": "starting"` with `message_count` still 0; the next three polls read
  `working`, `working`, `idle`. (Both that run's connections share one monotonic clock, so the
  stamps are directly comparable.) It never appeared in the first run, whose polling began well
  after the agent build had completed.

  **The run bounds the state's duration only loosely: longer than 6 milliseconds, and gone by the
  following poll 2.004 seconds later.** Nothing here measures it more finely, and no claim below
  depends on a finer figure. Two things do follow. The first contradicts a "too brief to see" reading
  outright: a poller on an ordinary cadence **can** sample `starting` — this one did, on its first
  attempt — so the registry must render the state rather than treat it as theoretical. The second
  holds under either reading: an *unseen* `starting` is still normal rather than evidence of
  anything, since the first run never caught it at all.
- **The row carries no kind.** A `waiting` row looks identical for clarify, sudo, and secret —
  the flattening KTD2's `unobserved` item type answers. Verified live: the row is exactly
  `{current, id, last_active, message_count, model, preview, session_key, started_at, status,
  title}`, epoch-seconds floats for the timestamps, `started_at` being session creation (not
  prompt start).
- **The plan's Grounding Evidence overstates `waiting`.** It records that `waiting` "derives from a
  gateway-global pending-prompt registry covering every prompt kind." At source — both revisions —
  `_session_live_status` (`server.py:8471` at the pin) consults only the `_block()` registry, which
  holds clarify, sudo, secret, terminal.read, preview.read, window.read, and mcp.setup. **Approvals
  live in a different structure** (`tools/approval.py`'s `_gateway_queues`) that the status
  function never reads. An approval-blocked session should therefore report `working` (its turn is
  in flight), not `waiting`. This is source-derived and could not be verified live (approvals are
  uninducible here — below); it is flagged for U9 confirmation on an approvals-enabled gateway.
  **Design consequence for U6/KTD2:** triggering `approval.pending` only "for rows reporting
  `waiting`" would never fire for the one kind it exists to fetch. The safe amended trigger is the
  union of a `waiting` status and an observed `approval.request` event for a driven session — plus
  KTD11's constraint unchanged (never fire it at a session whose agent liveness is unknown; a
  `working` row's agent is live by construction, so the amended trigger stays inside the rule).
- **A model note for registry rows:** the throwaway was created with one model and the gateway
  switched it per-turn to a fallback model, with the row's `model` field following the fallback.
  Row `model` is an observation, not configuration; registry rows should treat it as such.

## PC1 clause d — the approval surface

**Approvals are uninducible on this install as configured.** The default profile's persistent config
sets its approvals mode to `off` (read from the session's own `session.info`), and that branch
bypasses the *interactive prompt* at `tools/approval.py:4191-4192` at the pin. It does **not** bypass
every guard: three checks fire earlier in `check_all_command_guards` and each carries a source
comment saying it applies before the yolo / mode-off bypass — the hardline-command floor (`:4163`),
the sudo-stdin guard (`:4173`), and user-defined deny rules (`:4182`). A canary chosen to trip a
genuinely destructive pattern can therefore be refused by the hardline floor regardless of approval
mode, which is a second reason a canary may raise no `approval.request`. A canary command chosen to
trip the recursive-delete pattern executed twice with no `approval.request`, no queue entry, and no
prompt — `session.info` honestly reports the state (`yolo: true`, `approval_mode: "off"`). Since
the alternative was mutating the operator's live global config (forbidden by the run's safety
envelope, and it would have turned approval prompts on for every live surface of those profiles),
the approval legs were **not run**. What stands instead:

- **Live:** `approval.respond` is accepted cross-connection against a session the caller does not
  drive: `{"resolved": 0}` with nothing pending, both unaimed and with a bogus `request_id` (the
  wire revision ignores the parameter). These were evidence-only calls that granted and resolved
  nothing.
- **Live:** `approval.pending` is absent on the wire (`-32601`), so its row shape has no live
  answer today.
- **Source, at the pin (`7095e23eb`):** an `approval.pending` row is the queue entry's data dict,
  returned verbatim by `list_gateway_approvals` (`dict(entry.data)`, `tools/approval.py:2679`).
  **The shape is not fixed — four constructors feed it, and they do not agree.** All four get a
  `request_id`, which `_ApprovalEntry.__init__` sets by `setdefault` (`:2596`); everything else
  varies:

  | Constructor | `tools/approval.py` | Keys built |
  | --- | --- | --- |
  | dangerous-command / exec-ask | `:3493-3499` | `command` (redacted), `pattern_key`, `pattern_keys`, `description` (redacted), `allow_permanent`, `allow_session` |
  | blocking gateway approval (queue) | `:4570-4586` | the same six, `allow_*` conditional on a Smart-DENY override, plus `smart_denied` when that override applies |
  | code guard | `:5031-5038` | the same six, same conditional `smart_denied` |
  | MCP elicitation | `:5141-5146` | **only** `command`, `description`, `pattern_key`, `pattern_keys` — **no `allow_permanent`, no `allow_session`** |

  Three consequences bind U6 and U7. A pending row can arrive **missing** the `allow_*` keys a
  card's choice set is derived from, so a renderer must treat them as absent-not-false rather than
  index them. And `list_gateway_approvals` does **not** pass rows through the payload builder
  `_approval_request_payload` (`server.py:1918`) that derives `choices` — so a polled row and an
  evented request are not the same shape, and the poll feed must build its own choices or degrade by
  name.

  **The third is a redaction hazard, and it is the one that matters most here.** That same payload
  builder rewrites `command` through `_redact_approval_command` (`server.py:1928-1931`), which is
  `redact_sensitive_text(..., force=True)` — the forced redactor that, by its own docstring, "honors
  redaction even when `security.redact_secrets` is off" (`gateway/run.py:664`). **The
  `approval.pending` poll path never applies it.** Three of the four constructors pre-redact with the
  *unforced* `redact_sensitive_text`, and the MCP-elicitation constructor (`tools/approval.py:5142`)
  stores the raw `message` with no redaction at all. So a polled row's `command` can carry text the
  evented path would have masked. Talaria's own redaction guarantee is a recorder-gate promise, so a
  queue item built from a polled row must be treated as untrusted, unredacted gateway text — defanged
  and rendered literally like any other (R22/R23), and never written to a frame log on the assumption
  the gateway already cleaned it. **KTD12's answer: no start stamp of any kind exists on the row** —
  no `requested_at`, no
  monotonic mark, nothing. The `approval.request` *event* payload carries the entry's dict plus
  derived `choices` (`server.py:1918`), so it has no start stamp either.
  The "waiting ≥ observed span" floor is not a fallback for foreign approvals — it is the only
  honest age *any* client can render, including the one that watched the request arrive (whose
  floor simply starts at the arrival frame).
- **Source, at the pin:** `approval.request` events **do** carry the synthesized `request_id` —
  the entry's `__init__` sets it before the notify callback receives a copy of the same dict
  (`tools/approval.py:2596`, `:4030`, `:4056`), and the payload builder preserves unknown keys.
  Live confirmation deferred to U9 (needs an approvals-enabled gateway at the pin or later).
- The per-session approval structure is a **queue**, not a slot, at every revision examined —
  parallel subagents can block concurrently. Head-of-queue presentation (KTD9/R18) is the right
  model; "only one outstanding approval per session" was never structurally guaranteed.

## PC1 clause e — attach is steal, and what a steal recovers

Verified live on the wire revision except where a bullet says otherwise. Each label below is exact:
"verified live" means the captures show the arrangement; "source-derived" means the property holds
at the pin by construction but this run never exercised it.

- **`session.activate` rebinds the transport mid-turn — verified live, in one measured leg.**
  During the clarify leg, **A** activated the session while its turn was blocked on the prompt, and
  the displaced connection **B** was then measured at **zero** further session-scoped frames for
  that session. A received every subsequent event. The displaced client got no notification, no
  close, and no marker frame — silence is the only signal it gets. (`session.reclaimed` is not it:
  an explicit `session.close` did not broadcast one either — verified live; the broadcast fires only
  for the three backend reap reasons.)

  The probe *logged* a post-displacement count for this one leg only. The raw frame captures,
  however, independently confirm the same silence at **all four** ownership flips in the run: after
  B took the transport by submit, A received zero session-scoped frames until it re-activated; after
  A re-activated, B received zero until its own activate; and so on through the secret leg. So the
  measurement is one leg deep and the evidence is four flips wide. The leg scripted to observe an
  activate-steal *during an approval* never ran, because no `approval.request` could be raised on
  this install.
- **`prompt.submit` is also steal.** The submit handler rebinds the session's transport to the
  caller (`methods_prompt.py:337-341` at the pin; behavior confirmed live at the wire revision —
  a submit from B moved the whole event stream to B before any activate). The plan's Grounding
  Evidence lists create/resume/activate as the rebinding calls; submit belongs on that list.
  Consequence for KTD8: any client that merely *answers into* a session it does not drive via
  `prompt.submit` steals it as a side effect. Talaria's queue answers approvals via
  `approval.respond`, which does **not** rebind — **source-derived, not live-verified**. At the pin
  the only writes to `session["transport"]` are `tui_gateway/methods_prompt.py:341` (prompt submit),
  `tui_gateway/server.py:8664` (the shared live-session payload builder behind create, resume and
  activate), `server.py:8089`, `server.py:1194`, and `tui_gateway/compute_host.py:530` and `:632`;
  no `*.respond` handler is among them. This run could not exercise it: in **every** leg the
  connection that answered had activated the session moments earlier and so already owned the
  transport, so no respond was ever issued from a non-owning connection. OP3's inline-approval path
  not stealing rests on that source read, and U9 should exercise it on the wire. Navigation for
  every other kind goes through the KTD8 confirm before anything submits.
- **Hydration on today's wire: nothing.** Activate replies during a pending clarify, a pending
  sudo, and a pending secret each carried `status: "waiting"` and **no** `pending_clarify`, no
  `pending_approval`, and (as the pin also says) nothing for sudo/secret. At the pin the reply
  gains `pending_approval`/`pending_clarify` only. So the row of outcomes Talaria must render is:
  approval/clarify → cards after restart-onto-pin, resolved-failed latch today; sudo/secret →
  resolved-failed latch at every revision. The wire reply's shape today:
  `{inflight?, queued?, info, message_count, messages, messages_omitted, running, session_id,
  session_key, started_at, status}` (the pin adds `turn_started_at`).
- **Non-approval responds are aimable by `request_id` — verified live across connections, but not
  from a non-owning one.** A clarify raised while B held the transport was answered from A
  (`clarify.respond {request_id, answer}` → ok, turn resumed), and a sudo raised while A held it was
  declined from B with an empty password (`sudo.respond` → ok, turn resumed with the decline). So a
  request id raised on one connection is genuinely resolvable from a *different* connection. What
  the run did **not** show is a respond from a connection that did not own the transport: in both
  legs the answering connection had activated the session first. That the bridges resolve purely by
  request id, with no transport or session check, is **source-derived** — every non-approval respond
  bridge delegates to one helper, `_respond` (`tui_gateway/server.py:11629-11641`), which reads
  `params["request_id"]`, looks it up in the `_pending` registry, sets the answer, and never resolves
  a session or reads `session["transport"]` at all. On that
  source read the queue's named-session guard lives entirely in Talaria, since the gateway will let
  any authenticated client answer any pending prompt whose id it knows; U9 should confirm it on the
  wire before the guard's necessity is treated as measured.

## Prompt-kind payloads observed live

| Event | Payload keys observed |
| --- | --- |
| `clarify.request` | `question`, `choices`, `request_id` |
| `sudo.request` | `request_id` only |
| `secret.request` | `prompt`, `env_var`, `metadata` (carries the requesting skill's name), `request_id` |

The secret kind was genuinely induced (a skill's declared-but-missing environment variables fired
two concurrent `secret.request`s, 6 milliseconds apart). An empty `secret.respond` value resolves as
"skipped" without storing anything — no secret was typed at any point. Skipping the first left the
second pending, and the row held `waiting` for the entire 240-second window the status watcher then
polled at 1-second cadence — confirming that one visible `waiting` can hide several pending prompts
even within a single kind.

**No timeout was observed, and the run's timeline rules one out.** `_block`'s default expiry is 300
seconds and the secret bridge passes no override (`server.py:3460`, `:6245`; the sudo bridge does
override, at 120 seconds, `:6238`), so the unanswered secret would not have expired until 58.7
seconds after the run closed the session. The supportable claim is the 240-second hold with a hidden second
prompt; server-side expiry of a blocking prompt is **unverified** and is listed as such below.

## Cross-profile leg

**Deferred to U9, as the plan's U1 mechanism provides.** No second profile is paired on this
machine — per-profile credential acquisition (`talaria refresh-credential --profile`) is U2 work
(KTD5) and does not exist yet, and each profile's gateway mints its own credential, so the default
credential cannot be carried across. Nothing about the deferral is blocking: scoped routing,
steal, and hydration were all answerable on two connections to one gateway, and the cross-profile
claim v0.4 relies on (other profile processes are invisible to this process's listings) is
structural — `session.active_list` enumerates only in-process sessions.

## Listing shapes re-confirmed live

- `session.list` row: exactly `{id, message_count, preview, source, started_at, title}` — no
  profile field, no lifecycle field — confirming the requirements' dependency note on the wire.
- `session.create` reply: `{info, message_count, messages, session_id, stored_session_id}` — the
  durable identity (`stored_session_id`, and `session_key` in `active_list` rows) is available
  from the first reply, which is what KTD3's `(profile, durable_id)` registry key needs.

## What this run did not verify

Named plainly so nothing below reads as more than it is:

- Approval-side live behavior end to end: request event emission, `request_id` on the event, the
  pending-row shape on the wire, aimed cross-session resolution against a real pending entry, and
  the `working`-not-`waiting` status of an approval-blocked row. All are source-derived at the pin
  and land in U9's acceptance run, which needs an operator-designated approvals-enabled gateway.
- `terminal.read` / `preview.read` / `window.read` / `mcp.setup` transitions: not induced; their
  `waiting` derivation is shared source machinery with the three kinds that were verified.
- **How long `starting` lasts.** It was observed once and was gone by the next poll (above), which
  bounds it between 6 milliseconds and about 2 seconds. The run never sampled finely enough to say
  more, and nothing in this document depends on a finer figure.
- Anything about a second profile's gateway (deferred to U9, above).
- **A respond issued from a connection that does not own the transport.** In every leg that answered
  a *real pending prompt*, the answering connection had activated the session moments before, so
  "respond does not steal" and
  "the bridges apply no transport check" are both source-derived only. This matters because OP3's
  inline-approval design rests on them; U9 should exercise one respond from a genuinely non-owning
  connection.
- **Server-side expiry of a blocking prompt.** The unanswered second secret was still pending when
  the run closed the session, 58.7 seconds before `_block`'s 300-second default would have fired.
  Nothing in this run observed a timeout.
- **A steal during an approval.** The leg scripted for it never ran, because no `approval.request`
  could be raised on this install. (The post-displacement silence itself is *not* in this list: the
  probe logged a frame count for the clarify leg only, but the raw captures confirm zero
  displaced-connection events at all four ownership flips — see the attach-is-steal section.)

## Safety envelope of the run

The operator-designated disposable workspace and throwaway-session rules were confirmed before the
first mutating call. The run created two throwaway sessions (both titled as probes, both with
`close_on_disconnect` set), drove them with a canary recursive-delete of a directory the run
itself created inside the session scratchpad, declined the sudo prompt with an empty password,
skipped the secret prompts with empty values, closed both sessions, and verified the serving
process held no other live session at any point. All respond calls against anything not created
by the run were evidence-only (`resolved: 0`). Captures were scanned for the credential after the
run: none present.
