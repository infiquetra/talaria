---
date: 2026-08-16
topic: talaria-v0-4-fleet-turn
maturity: requirements-ready
source: docs/ideation/2026-08-02-talaria-product-shape-ideation.md — survivors 1, 3, 5, 6, 7
---

# Talaria v0.4 fleet turn — requirements

## Summary

Talaria v0.4 turns the client from "one session in view" into "one Hermes install in view". The
root object of domain state becomes a registry of every session the connected install runs, with
the focused session as a cursor over it rather than the container of everything. Startup probes
name every seam that is present, absent, or degraded, so a missing subsystem renders as a named
absence instead of as empty data. And every human-facing blocking prompt from every session — not
just the focused one — lands in one typed needs-you queue ordered by how long it has waited.

The focused session's surfaces — transcript, composer, prompt cards, sub-agent rows — are
unchanged. Two rules bind every new surface: every fleet value carries its source and age and
renders stale as stale, and no operator action updates displayed state until the gateway's own
events confirm it.

This document fixes the product and safety contract. It is input to `/plan`, not direct
authorization for `/work`: the implementation plan must close every planning obligation below and
preserve every requirement-to-verification mapping before source work begins. The v0.1 requirements
(R1–R40 of the prototype document) and the block-markdown requirements remain binding; nothing
below amends them except where explicitly stated.

## Problem Frame

Three releases sit on one axis. v0.1 built the transport seam and one live session, v0.2 made that
session answerable and readable, v0.3 made it confirm what it just did — single-session craft,
compounding well. The fleet axis the product ideation names as Talaria's reason to exist has no
code in it: five of the seven survivors in the ideation's own MVP tier are unstarted or barely
started, and the two later tiers are untouched apart from record-and-replay.

The current architecture does not merely lack fleet features; it discards their raw material on
arrival. The cross-talk guard in `_apply_event` (`talaria/domain/state.py`) drops every event that
does not belong to the focused session, counting them in `cross_session_events_ignored` — correct
under v0.1's R37, and v0.3 extended it so a background session's unknown event cannot corrupt the
visible transcript. The guard exists because background traffic **arrives**; today the state tree
has nowhere to put it. A registry gives it somewhere to go.

The cost of doing nothing is the one v0.1's problem frame named for delegated sub-agents, now one
level up: a blocked session waits to be discovered rather than announcing itself, and discovery
happens whenever the operator next thinks to look. The ideation's live probe found exactly this —
several tasks blocked across different profiles with no terminal surface anywhere reporting it.
Meanwhile the queue's foundations half-exist: the prompt registry already survives focus switches
with session-qualified ids (`focus_session`, `talaria/domain/state.py:409` — kept precisely so a
switch cannot orphan a control the gateway still holds open), and rendering already filters it to
the focused session. What is missing is the routing that lets background prompts in, the summary
state that says what each session is doing, and the surface that shows any of it.

An adversarial review of this document's first draft (2026-08-16, in-session, against the current
tree, the ideation, and the Q1 ruling) found one missing security requirement, one unsized scope
risk, and four precision gaps; this revision incorporates all thirteen of its findings and three
operator clarifications — never-observed is not stale, an event without trustworthy session
identity needs a safe visible path, and the needs-you summary's space is reserved by layout. A
second revision (2026-08-17, on a four-engine doc-review panel's findings against the
implementation plan) amended R18 (the gateway now synthesizes approval request ids — an observed
id is sent with the answer), AE11 (an unrecoverable-kind item's floor is a latched visible
failure, never a silent dead end), R14 (the queueable-kind enumeration and the row-latched path
for unrenderable kinds), and the `session_id` field name in the dependency notes.

## Key Decisions

**Answering a blocked agent is driving, not authoring — ruled by the operator, 2026-08-16.** The
ideation's open boundary question Q1 is settled: answering an approval, clarification, sudo,
secret, or terminal-read prompt is the same category of act as steering a session or typing a
message. Constraint 7 — Talaria reads agent state and never authors agent identity — stands
unchanged; the ruling rests on shipped precedent, since Talaria has answered all four human-facing
prompt kinds for the focused session since v0.2, and the queue changes where the answer is aimed
from, not what is answered. Recorded with rationale and revisit condition in
`docs/engineering-journal/DECISIONS.md` under 2026-08-16. This unblocks ideation survivor 1.

**The registry is the root object; the focused session is a cursor.** This takes up exactly the
deferral v0.1 recorded — "one session in view; the session-registry surface is deferred …
deferring it must foreclose nothing" — and forecloses the opposite direction instead: every fleet
feature after v0.4 is a join against the registry, and each release that adds single-session
surface on top of a single-session root raises the cost of the pivot. An unfocused session is a
bounded summary row, never an accumulating transcript.

**The needs-you queue is one flat typed list ordered by wait age.** Not topology, not per-session
tabs, not a board. Five of six ideation frames converged on this shape independently — the
strongest signal in the run — and the flat-at-decision-time shape is the industry prior art the
ideation cites. v0.4 populates it from the gateway's blocking prompts only; the item type must
admit Kanban and pane-manager sources later without restatement, because retrofitting a type is
rework but retrofitting a discipline is failure. One deliberate softening against the source is
declared here: survivor 1 asks for a typed answer per row, and this document ships
navigate-to-card as the floor with inline answering as PC4's per-kind option — every answer must
ride the single shipped answer path, and the card is where that path's affordances (masking,
hints, focus) already live.

**Seam probes extend the v0.1 compatibility baseline rather than replacing it.** The pinned
read-only probing discipline stands: `READ_ONLY_METHODS` (`talaria/domain/compat.py:316`) is
probed, everything else is evidence-only, and no mutating method is ever invoked to discover
whether it exists. What v0.4 adds is naming: which seams of the install are present, absent, or
degraded, per connection, with the consequence stated — which feature is off and why. The
ideation's load-bearing finding binds here: "the gateway" is two processes, the terminal gateway
Talaria dials and a separate runner that owns the HTTP API and the Kanban dispatcher, so a client
that models "connected" as one boolean will eventually render an absent subsystem as zero work. A
fabricated zero is the failure mode this unit exists to make structurally impossible.

**Source-and-age and fire-and-observe are standing rules, not panes.** Ideation survivors 6 and 7
ship as requirements binding every fleet surface (R20, R21) rather than as features. Both have
shipped precedent in miniature: the age-out machinery and unknown-capability states for the first,
the confirmed-interrupt-then-sweep discipline for the second. The ideation's warning stands —
neither can be retrofitted onto a surface people have already learned to trust.

**Connection topology is deliberately undecided in this document.** One app-global remote-mode
connection hosting sessions across profiles, versus one connection per profile gateway, is the
first design decision of the plan (PC1) and is pending live verification against the running
install. The requirements below are written to hold under either answer; anything that could not
is called out where it occurs.

## Actors

- A1. **The operator** — one person, running Hermes agents against their own repositories, with
  Hermes's terminal UI available as the alternative at all times.
- A2. **The Hermes terminal gateway, or gateways** — the authenticated WebSocket attach surface(s)
  of one Hermes install, dialled and never launched, per ADR-0001. Whether one connection or
  several is PC1's decision. Scope is one install and all of its profiles — never multi-host
  (ideation constraint 3).
- A3. **Background sessions** — sessions of the install that are not currently focused. Talaria
  reads their state, renders it in summary, routes their prompts into the queue, and never authors
  it.
- A4. **The install's other processes** — the runner that owns the HTTP API and the Kanban
  dispatcher, and any other seam the probe set names. In v0.4 they are probed and named, never
  driven.

## Requirements

**The registry**

- R1. The root object of domain state is a registry of sessions keyed by profile and session id,
  covering the connected install (PC1's scope valve governs how much of the install v0.4 stages
  if the topology verification forces the per-profile branch). The focused session is a selection
  within the registry, not the container of it. The registry is domain state and imports no
  terminal framework (ADR-0002).
- R2. The focused session's surfaces and behavior are unchanged from v0.1–v0.3: transcript,
  composer, prompt cards, sub-agent rows, and every requirement that governs them. Switching focus
  is a cursor move; the arriving session renders through the v0.2 resume path — history rendered,
  withheld history named, never silently blank.
- R3. An unfocused session holds a bounded summary: lifecycle (for example streaming, idle,
  waiting on the operator, disconnected), what it is waiting on, and the source and age of the
  last event that changed it. An unfocused session never accumulates an unbounded transcript; its
  memory cost is a row, not a session. The `/sessions` picker's rows render this same summary —
  lifecycle, waiting-on, age — so the registry powers the shipped surface rather than growing a
  parallel one.
- R4. Events are routed to their session's registry entry. The focused-session cross-talk guard's
  *protection* survives — another session's event still cannot mutate the focused transcript — but
  its *discard* does not: a background session's event updates that session's row. An event for a
  session the registry does not know is surfaced per v0.1 R5, never silently dropped, and its
  session becomes known rather than ignored.
- R5. The registry is seeded from `session.list` at attach and maintained from session-change
  notifications and observed traffic. A listing the gateway refuses, or one that predates a
  reconnect, is marked stale rather than rendered fresh (R20 binds here).
- R6. Focus movement is correct under churn. The two recorded `focus_session` defects — withdrawn
  approvals surviving into the wrong session's screen, and the in-flight bookkeeping the prompt
  registry depends on being disarmed by a switch — are fixed as part of this unit, because the
  queue makes focus movement routine rather than occasional. The existing refusal rule (a switch
  is refused while an answer is in flight) is kept and reported, and queue navigation obeys it.
- R7. Recording and replay carry the registry. Frame logs already record every session's frames;
  replaying a multi-session recording reconstructs the same registry, queue, and focused
  projection deterministically, with no socket open (v0.1 R30/R40 inheritance). Replay derives
  the focused session by a defined rule — the recorded landing replies when the recording carries
  them, else the first session named on the wire, today's adoption rule — and the gate asserts
  the derivation (PC5).
- R8. Nothing in R1–R7 assumes the topology answer. Per-connection state — credentials, epochs,
  reconnect, redaction — stays per-connection exactly as v0.1 built it, however many connections
  PC1 decides there are.
- R25. An event without trustworthy session identity — no session id, a malformed one, or one
  that conflicts with what the connection can own — is never routed to a row by guess. It
  surfaces visibly on the connection-scoped channel exactly as v0.1 R5 requires (the session-less
  `ProtocolErrorFrame` already takes this path), is counted, and creates or mutates no registry
  row. R4 governs the identified-but-unknown session; this rule governs the unidentifiable one.

**Seam probes**

- R9. Startup names, per connection, the seams that are present, absent, or incompatible,
  extending the pinned compatibility baseline. Probing invokes read-only methods only; mutating
  methods are never probes (v0.1 R34 inheritance). Probe results are re-validated on reconnect.
- R10. A feature whose seam is missing is off *by name*: the surface states which seam is absent
  and which capability that disables. Absence of a data source renders as a named absence, never
  as empty data — an empty board with no dispatcher reads "no dispatcher", never "zero work".
- R11. Probe classification distinguishes endpoint-absent from parameter-invalid: a failed probe
  whose request carried a parameter is re-asked bare before the capability may be called absent.
  This generalizes the recorded `absent_capability` misdiagnosis, which blamed a gateway's version
  for a mistyped profile name and remains open in the queue of deferred work.
- R12. A seam that degrades mid-session updates the named surface with source and age; the probe
  story is a live claim, not a startup banner that goes stale the moment it scrolls.

**The needs-you queue**

- R13. One typed item — reference (session and request key), source, kind, prompt text, allowed
  answers, and age — describes everything that needs a person. The type admits sources beyond the
  gateway (Kanban blocked state, pane-manager blocked state) without restatement; v0.4 populates
  gateway prompts only.
- R14. Every outstanding human-facing blocking prompt from every registry session appears in the
  queue: approval, clarification, secret, sudo — the four kinds Talaria renders cards for, which
  is what makes them queueable under R17's resolvability rule. A blocking prompt of a kind Talaria
  cannot render anywhere (the running gateway has grown three such kinds since the pin) resolves
  on its session's registry row as a named, latched state and is never a queue item, exactly as
  the terminal-read bridge's failure contract already works; a foreign session whose waiting kind
  is not yet knowable enters the queue as an unobserved item under AE11's resolution rule
  (amended 2026-08-17). The focused session's own prompts are included —
  the count is the install's whole truth, and the card the operator is already looking at is one
  of its rows. Terminal-read stays a machine-answered bridge (`answer_terminal_read`,
  `talaria/ui/app.py:2786`); a terminal-read Talaria cannot serve — a background session is the
  structural case, since Talaria projects no view of it — fails visibly per the bridge's failure
  contract and is never left silently registered (this takes up the recorded
  unavailable-projection defect). The failure surfaces on its session's registry row and in that
  session's transcript, never as a queue item: the queue holds only resolvable items (R17).
- R15. The queue is flat and ordered by wait age, oldest first. No topology, no grouping by
  session or profile.
- R16. A summary — how many items wait, and the oldest item's age and session — is discoverable at
  a glance without entering any mode. Its space is **reserved by layout**: a permanently present
  row, or a permanently present segment of an existing row, claimed at first mount and rendering
  an explicit empty state when nothing waits. The summary is never conditionally mounted, so its
  presence, absence, or change structurally cannot reflow the transcript or move any widget's
  height (the height-stability rule the caret work established).
- R17. The queue's detail view is a modal drill-down consistent with the shipped picker
  conventions — arrows move, typing filters, `enter` acts, `escape` leaves, the composer's draft
  survives. Selecting an item focuses its session and the card takes the caret exactly as v0.3
  shipped for the focused session; a queue row is never a dead end that only the mouse can
  resolve. Whether any kind is answerable inline from the queue itself is PC4's decision; if any
  is, its answer goes through the same single answer-path function both existing paths use.
- R18. Answering is fire-and-observe. A row in flight renders as requested-with-age; a row clears
  only on gateway-confirmed resolution or expiry; an ambiguous approval outcome settles and
  latches per the recorded decision, never restores. Within one session, approvals present in
  gateway queue order: the queue aims answers at a session's head approval and never presents a
  session's second approval as independently answerable while its first is outstanding. When the
  gateway supplies a request id for the head approval — the running revision synthesizes one and
  `approval.respond` accepts it, a drift from the pinned read this document originally recorded —
  the answer carries that id, because the gateway removes queue heads on timeout and interrupt
  without emitting anything, and an uncorrelated answer can authorize a command the operator was
  never shown. When no id is observed, the shipped uncorrelated-approval refusal and deny-all
  fallback apply unchanged. (Amended 2026-08-17 on the doc-review panel's blocking finding.)
- R19. Expiry and withdrawal are visible twice: the expired prompt leaves its persistent
  indication in its own session's transcript (v0.1 R8 inheritance), and the queue row clears with
  the summary count updating in the same render boundary.

**Standing rules**

- R20. Every fleet-surface value — registry rows, probe results, queue ages, the summary line —
  carries the source it came from and the age of the event that last changed it. A value whose
  source has dropped renders "stale since", never a frozen number presented as current, and never
  a fabricated zero. Ages derive from the clock domain that stamps frames — never the render-time
  wall clock — so a replayed recording reproduces every age exactly.
- R21. No cross-session verb updates displayed state optimistically. Every mutation renders an
  explicit requested state with visible age and changes displayed state only when the
  corresponding gateway event arrives or the reference re-resolves. Acknowledged,
  delivered-but-not-observed, and never-delivered are three distinguishable outcomes, as the
  shipped interrupt discipline already treats them.
- R22. The v0.1 security boundary (R9) extends to every new surface: no credential-bearing or
  sensitive respond value reaches a registry row, queue row, probe diagnostic, frame log,
  transcript export, or status payload. A queue row renders a respond value only if the gateway
  itself offered it, per the recorded decision. Background-session traffic passes the same
  redaction boundary as focused traffic — the recorder already sees every frame, so the deny-set
  already governs it; this requirement makes that inheritance explicit rather than assumed.
- R23. Every surface this document introduces — registry summaries, queue rows, the needs-you
  summary, probe and seam lines, drill-down rows — renders gateway-supplied text only through the
  established untrusted-text boundary (`defang` plus `literal_text`, ADR-0005's rule, exactly as
  the prompt cards do today). No markup interpretation, no control sequences, no actions or
  links; markdown semantics remain transcript-only under the block-markdown allowlist.
- R24. Never-observed is not stale, and both are named. Three display classes, each distinct on
  screen: **live** (the source is current), **stale** (the source was current and stopped —
  "stale since", per R20), and **never observed** (no observation exists — a seeded row whose
  lifecycle no event has confirmed, a seam never probed, a gap in coverage). The third renders as
  not-observed or a named absence — never as zero, never as idle, and never as stale-since-nothing.
  The ideation's own form is the model: "gap: not observed 14:02–14:31".

## Key Flows

- F1. **Startup on a working install.** _Trigger:_ the operator launches Talaria against an
  install with several sessions and one absent seam. Talaria attaches, verifies the pinned
  baseline, probes the seam set read-only, seeds the registry from `session.list`, renders the
  focused session exactly as v0.3 does, shows the needs-you summary, and names the absent seam
  with the feature it disables. **Covers R1, R2, R5, R9, R10, R16, R20.**
- F2. **A background session needs the operator.** _Trigger:_ a session that is not focused
  raises an approval. The event routes to its registry row instead of being discarded, the row's
  lifecycle reads waiting-on-you, the summary increments with the item's age, the operator opens
  the drill-down, selects the item, focus moves, the card takes the caret, the answer is sent
  fire-and-observe, the gateway confirms, and the row clears everywhere in the same boundary.
  **Covers R4, R13, R14, R15, R16, R17, R18, R19, R21.**
- F3. **A focus switch under churn.** _Trigger:_ the operator switches sessions while prompts are
  outstanding in both. The arriving session renders through the resume path; the outgoing session
  keeps accumulating as a summary row; prompt bookkeeping survives re-keyed; a switch attempted
  while an answer is in flight is refused and says so. **Covers R2, R3, R6.**
- F4. **A source drops.** _Trigger:_ a connection closes while background sessions have queue
  items. Affected rows and queue items mark themselves stale with age rather than freezing;
  reconnect re-seeds from `session.list`, re-validates probes, and reconciles without duplicating
  transcript entries or queue rows. **Covers R5, R8, R12, R20; inherits v0.1 R35/F6.**
- F5. **Replay of a fleet recording.** _Trigger:_ the operator replays a recording whose frames
  interleave several sessions. The registry, queue, summary, and focused projection reconstruct
  deterministically with no socket; mutation controls are inert per v0.1 R40. **Covers R7.**

## Acceptance Examples

- AE1. **When a background session's event arrives — including an unknown type**, its registry row
  updates, the focused transcript does not change, and nothing is silently dropped: the discard
  counter's role is replaced by routing, asserted by a test that fails against the current
  discard-and-count behavior. An event with no usable session identity surfaces on the
  connection-scoped channel, is counted, and creates no row. **Covers R1, R3, R4, R25.**
- AE2. **When one background session holds two approvals and the operator answers from the
  queue**, the answer aims at that session's head approval; the second row is shown waiting, not
  answerable ahead of its turn; an outcome that comes back "gateway not waiting" settles and
  latches rather than restoring. **Covers R14, R15, R18.**
- AE3. **When a probed seam is absent, and when a probe's parameterized request fails**, the
  absent seam is named with its disabled feature and no surface renders empty data as zero; the
  parameterized failure is re-asked bare before absence may be claimed. **Covers R9, R10, R11.**
- AE4. **When a source stops updating**, every value derived from it renders "stale since" with
  source and age within one re-validation interval, and no frozen value is presented as fresh. A
  value with no observation behind it renders as not-observed — never as zero, idle, or stale.
  **Covers R12, R20, R24.**
- AE5. **When an answer is sent and no confirming event ever arrives**, the row renders
  requested-with-age indefinitely, is never shown resolved, and a late confirmation resolves it
  exactly once. **Covers R18, R21.**
- AE6. **When focus switches rapidly across sessions with outstanding prompts**, no withdrawn
  approval hedge leaks into the wrong session, no in-flight bookkeeping is disarmed, and every
  outstanding prompt is answerable when its session is refocused — asserted against the two
  recorded defect mechanisms. **Covers R2, R6.**
- AE7. **When synthetic credential-bearing prompt traffic crosses the socket for a background
  session while recording is active**, no sensitive value appears in any registry row, queue row,
  probe diagnostic, frame log, export, or status payload, and each withheld value leaves a marker.
  When a background session's title or prompt text carries a Rich markup literal, a raw ANSI
  escape, or HTML, every new surface shows it as visible literal text — nothing styles, executes,
  opens, or vanishes. **Covers R22, R23.**
- AE8. **When the same multi-session recording is replayed twice**, both runs produce identical
  registry, queue, and focused-projection state at every checkpoint — including the derived focus
  and every rendered age — with no socket open. **Covers R7, R20.**
- AE9. **When the queue goes from empty to one item to many and back**, the summary renders in
  space reserved since first mount — the empty state included — never changes any widget's
  height, and the transcript never reflows because of it. **Covers R16.**
- AE10. **Operator-driven live leg.** With live sessions from at least two profiles on a real
  install — or, under PC1's staged scope, at least two concurrent sessions of the configured
  gateway — a real background prompt is discovered from the summary, opened in the drill-down,
  and answered, and the confirming clear is observed — driven by hand, recorded as release
  evidence. v0.3 shipped with no unit gated on a live drive; this release does not repeat that.
  **Covers F2 end-to-end.**
- AE11. **When every queue kind is resolved using only the keyboard in a headless run**, each row
  reaches a keyboard-reachable end: navigation to its card with the caret landing on the control,
  or — for an item whose kind cannot be recovered after a confirmed attach — a latched, visible
  failure on its row and its queue item, per the terminal-read settle precedent. No kind requires
  the mouse, and no path ends silently. (Amended 2026-08-17: the gateway exposes only a flattened
  `waiting` for sessions other clients drive, and attach hydrates only approvals and
  clarifications, so an unknown-kind item's honest floor is a visible latched failure, never a
  silent dead end.) **Covers R17.**

## Requirement Traceability

The implementation plan must assign every row below to concrete units and executable evidence; a
range in this table is compression, not permission to omit an individual requirement.

| Requirements | Primary evidence in this document |
| ------------ | --------------------------------- |
| R1–R8, R25   | F1, F3, F4, F5; AE1, AE6, AE8     |
| R9–R12       | F1, F4; AE3, AE4                  |
| R13–R19      | F2; AE2, AE5, AE9, AE10, AE11     |
| R20–R21, R24 | F1, F4; AE4, AE5, AE8             |
| R22–R23      | AE7                               |

## Scope Boundaries

Out of v0.4:

- Kanban and pane-manager queue sources. The item type admits them (R13); nothing populates them.
- The telemetry-exporter fleet header (ideation survivor 2) and the append-only local event log
  (survivor 4). Deferred, not rejected; the log is Talaria's first durable state and deserves its
  own release.
- Spawning work as sessions, the three steering verbs, and any board or dispatch surface
  (survivors 8, 9, 11, 12). The registry is their prerequisite, not their delivery.
- Attention budgeting and the exception-strip cockpit (survivors 13, 14). The needs-you summary is
  one count with one age; it is not the exception strip, and building the strip before the queue
  has real traffic would tune thresholds against nothing.
- Theming, readability, and transcript visual polish. Deliberately reserved as the next release's
  spine: the operator drove v0.3 by hand and is holding interface feedback for exactly this work.
- Linux exercising of any of it. Parked by operator ruling, 2026-08-16; the claims rule stands —
  nothing is claimed for platforms the matrix does not list.
- Multi-host fleets (ideation constraint 3) and publishing to the Python Package Index
  (unchanged deferral).

## Dependencies and Assumptions

- **The topology facts are pinned reads from 2026-08-02 and must be re-verified live before the
  plan hardens (PC1).** The ideation's mechanism — app-global remote mode, one connection hosting
  sessions across profiles, `session.create`/`session.resume` taking a profile parameter — was
  read at a Hermes revision six-plus weeks stale by this repository's own drift precedent. Today's
  shipped topology is one gateway address per profile from Talaria's own config. PC1's scope
  valve bounds the blast radius of whichever answer comes back.
- **Whether one connection observes other sessions' blocking prompts is the load-bearing unknown.**
  Background traffic demonstrably arrives on the focused connection — the cross-talk guard counts
  it — but whether that includes `*.request` prompt traffic and its expiries for sessions this
  connection did not open has not been established. PC1's live verification settles it, and the
  answer decides how the queue is fed under either topology.
- **`session.list` carries identity, not liveness.** The decoded summary
  (`talaria/domain/session_list.py:44`) holds `session_id` (decoded from the wire row's `id`),
  title, preview, started-at, message count, and source — no lifecycle field. R3's lifecycle summaries therefore derive from routed traffic, not
  from the listing; if the pinned revision offers a richer listing, planning may use it, but
  nothing here assumes it.
- **The prompt-correlation floor exists.** The prompt registry stores the session, synthesized
  approval ids are session-qualified, and `approval.respond` sends the session id — so aiming an
  answer at a background session is a wire capability, verified for approvals at the pinned
  revision. The other three bridges' respond parameters must be re-verified for cross-session use
  in PC1.
- **ADR-0001, ADR-0002, and ADR-0003 bind unchanged.** Talaria dials gateways it never launches,
  the registry lives below the framework boundary, and Hermes behavior is re-encoded from pinned
  reads, never ported.

## Planning Closure Obligations

These are HOW decisions, not optional questions. `/plan` must resolve each from cited evidence and
record the verification that can falsify it; an implementer must not choose a default ad hoc.

- PC1. **Settle the connection topology first, by live verification against the running install at
  a pinned revision.** One app-global remote-mode connection versus one connection per profile
  gateway. Establish, with recorded evidence: (a) whether the running gateway accepts
  remote-mode session creation and resume with a profile parameter; (b) whether one connection
  observes other sessions' events **including blocking-prompt requests and expiries**; (c) whether
  each of the four respond bridges can be aimed at a non-focused session; (d) what `session.list`
  actually carries per row at the pin — by name, whether any field identifies a session's
  **profile**, since the decoded summary today carries none and the remote-mode branch has no
  other source for the registry key's profile half — and what `sessions.changed` actually
  carries; (e) the credential consequence of each branch, since the credential file is a
  single-entry `url`-plus-`token` shape today, so the per-profile branch implies multi-credential
  acquisition, storage, and rotation that must be sized, not inherited. Record the rejected
  branch's rationale in the plan. **The scope valve:** if verification forces the per-profile
  branch, the plan must either size the multi-connection credential and lifecycle work explicitly
  or stage v0.4 as registry-over-the-configured-gateway — every session of one gateway, with
  multi-profile coverage deferred to a stated release — recorded as a decision, never drift.
  Every other obligation may proceed in parallel only to the extent it is topology-neutral.
- PC2. Define the unfocused-row summary: its fields, its lifecycle vocabulary, its memory bound,
  the bound on registry row count and the retirement rule for sessions the install no longer
  reports, and the re-keying rules that satisfy R3 and R6.
- PC3. Fix the probe-set membership and classification (present, absent, incompatible, degraded,
  parameter-invalid) at the pinned revision, and the re-validation cadence R12 requires. Bound
  probe traffic — read-only, low cadence, never per-render — and state probe behavior under
  replay: recorded probe replies replay; a seam the recording never observed is marked
  never-observed (R24), and no socket opens.
- PC4. Design the queue surface within R16's reserved-layout rule: which permanently present row
  or segment carries the summary, its empty state, keybinding, drill-down layout, and the
  per-kind resolve affordance — navigate-to-card as the floor, inline answer only through the
  existing single answer-path function.
- PC5. Restate the replay gate for fleet state: what the gate asserts about registry and queue
  reconstruction, the focus-derivation rule, and age reproduction (R7, R20, AE8), extending the
  existing determinism checks rather than duplicating them.
- PC6. Specify the focus-churn repair (R6, AE6) against the two recorded defect mechanisms, and
  the refusal-reporting path queue navigation uses.

## Sources and Research

- `docs/ideation/2026-08-02-talaria-product-shape-ideation.md` — survivors 1, 3, 5, 6, 7 are
  load-bearing; constraint 3 bounds scope; constraint 7 stands under the Q1 ruling; survivor 5's
  Hermes-side mechanism is the pinned read PC1 re-verifies.
- `docs/engineering-journal/DECISIONS.md` — the Q1 ruling (2026-08-16); "An approval-kind outcome
  that comes back 'gateway not waiting' or a definite not_sent never restores; it settles and
  latches"; "One function decides what an answered prompt may claim, and both answer paths go
  through it"; "A respond value reaches the transcript only if the gateway itself offered it";
  the modal-picker decision that overturned the anti-modal rule.
- `docs/engineering-journal/QUEUED.md` — the open `absent_capability` misdiagnosis (R11), the
  two recorded `focus_session` defects (R6), and the unavailable-projection terminal-read defect
  (R14).
- `talaria/domain/state.py` — `focus_session` (line 409) and its kept-across-switches prompt
  registry; the `_apply_event` cross-talk guard and `cross_session_events_ignored` counter R4
  replaces with routing.
- `talaria/domain/session_list.py:44` — `SessionSummary`, the fields `session.list` is decoded to
  today.
- `talaria/domain/compat.py:316` — `READ_ONLY_METHODS`, the probing discipline R9 extends.
- `talaria/ui/app.py` — the `/sessions` picker's fresh `session.list` fetch (line 3799) and
  `answer_terminal_read` (line 2786).
- `docs/analysis/2026-08-02-hermes-reconciliation-rules.md` — RR-28: `approval.request` carries no
  request id, the fact behind R18's head-of-queue aiming.
- `docs/releases/v0.3.0.md` — the no-live-drive limit AE10 exists to not repeat.
- Operator rulings, 2026-08-16: the Q1 ruling; Linux parked; interface-polish feedback held for
  the theming-and-readability release.
