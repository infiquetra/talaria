---
title: v0.2 — answerable prompts and the session story
type: feat
status: active
date: 2026-08-08
origin: docs/plans/2026-08-08-v0-2-session-handoff.md
---

# v0.2 — answerable prompts and the session story

## Summary

v0.2 takes two of the four candidate spines the [v0.2 handoff](2026-08-08-v0-2-session-handoff.md)
laid out: **make the interface answerable** (the P0 focus defect plus its satellite items) and
**make `--resume` mean what it says** (history rendering, extended into a session switcher pulled
from the v0.1 brainstorm's deferred list). The third chosen spine — block-level markdown — is
deliberately **not** in this plan: it is a requirement change with an open architectural question
and gets its own `/brainstorm` first. The operator chose this scope on 2026-08-08.

## Problem Frame

v0.1.0 is released and its verdict is READY, but three defects found by driving the interface live
sit in the queue, and the worst of them can lose an operator's data: a typed answer to a blocking
prompt lands in the composer as plain chat text, one `enter` from the transcript. Separately,
`--resume` lands in the right session and renders an empty transcript — the gap most likely to be
noticed first by anyone who installs v0.1.0. Every one of these shares one shape: *the machine does
the right thing when driven correctly, and nothing tells the operator what correct is.* v0.2's theme
is closing that shape — after it, Talaria can be recommended to a second person.

## Requirements

- **R1.** From anywhere in the interface, one keypress moves the caret to the first unanswered
  prompt card's control, and every prompt card carries a hint line naming the keys that operate it
  (the picker dialog's convention, `talaria/domain/selection.py:40`).
- **R2.** A focused prompt control is visibly focused — legible against the default terminal theme,
  not only `Button`'s reverse video and the agent-row tint that exist today.
- **R3.** The operator can decline any outstanding attended prompt (approval, clarify, sudo,
  secret — `terminal_read` is auto-answered and never renders a card) without waiting for expiry.
  Decline sends, per kind, exactly what the gateway's own client sends on its escape path: the empty
  field value for clarify/sudo/secret (`ui-tui/src/app/useInputHandlers.ts:119`, `:128` at Hermes
  `7f4d15515`), and the explicit `{choice: "deny"}` for approvals (`useInputHandlers.ts:180`). An
  empty approval choice is **not** a decline: the gateway's approval consumer blocks only `None` and
  `"deny"` and returns *approved* for any other resolved choice (`tools/approval.py:3291`, `:3320`),
  so an empty-choice "decline" would authorize the command. A declined prompt clears like an
  answered one and follows the same outcome discipline as an answer: cleared before the decline is
  sent (per the recorded decision "A prompt is cleared before its answer is sent"), restored only on
  a definite `not_sent` (`talaria/domain/state.py:708` — the gateway is still waiting in that case),
  and latched against restoration once any other outcome is known.
- **R4.** A **confirmed** `session.interrupt` (F4) also declines every outstanding prompt of the
  focused session. An interrupt with a lost outcome changes nothing, matching v0.1's R4/AE8
  discipline in `interrupt_live` (`talaria/ui/app.py:1401-1427`; `action_interrupt` itself is the
  short dispatcher at `:1251-1256`).
- **R5.** Whenever the caret is not in the composer, the status region names what holds it. No
  widget's height changes in any focus state, and no row is added to or removed from the `#body`
  stack (`talaria/ui/app.py:860-871`) — the constraint recorded in `talaria/ui/composer.py:181-189`.
- **R6.** `talaria --resume` and `talaria --session <id>` render the resumed session's history: the
  `messages` array of the `session.resume` reply appears as committed transcript entries before any
  live event, ordered before the live stream's entries. When `messages_omitted` is true, the
  transcript says so explicitly, naming the withheld count from `message_count` — a withheld history
  must not render as a complete short one. The gateway's omission path delivers an **empty**
  `messages` array with the full `message_count` (`tui_gateway/methods_session.py:494`), so the
  notice names `message_count` itself; if a future gateway delivers partial history, the notice
  names the difference. Seeded history strictly precedes any live event applied after landing — an
  ordering barrier (KTD2) that U6's transport test asserts. The reply's durable identity
  (`resumed` / `session_key`, `tui_gateway/methods_session.py:498`) is retained alongside the
  runtime session id — they can differ, and discarding the durable one leaves the picker (R7)
  nothing to match on.
- **R7.** `/sessions` opens a modal session picker fed by `session.list`. Choosing a session
  switches to it through the same path startup uses — `session.resume` → `_land_session` — and the
  switched-to session's history renders (R6 applies). Picker rows are identified by the **stored**
  session id `session.list` returns (`tui_gateway/methods_session.py:197`), and the focused-session
  highlight matches on the durable identity R6 retains — not the runtime id, which can differ.
  Shadowing the gateway's own dispatchable `/sessions` command is deliberate (KTD6).
- **R8.** Before the switcher lands, `focus_session` (`talaria/domain/state.py:274-299`) is
  hardened: `withdrawn_approvals` resets on switch; a switch is **refused while any prompt answer
  RPC is in flight** — a late outcome resolving after a switch mutates the newly focused session's
  transcript (`talaria/ui/app.py:1623`), and the refusal window is one round-trip;
  `flushed_prompt_ids` is retained across switches with synthesized approval ids qualified by
  session (the synthesis counter restarts per landing, `talaria/domain/state.py:1321`, so
  unqualified retention would collide); and `prompt_view` (`talaria/domain/projection.py:346`) gains
  the session filter whose absence lets a foreign session's prompt render.
- **R9.** A 404 on an admin request that carried a `?profile=` parameter is only reported as
  `absent_capability` when the *bare* path also 404s; otherwise the error names the unknown profile.
  The misdiagnosis fires today from `_http_error` (`talaria/transport/admin.py:390-407`), which
  never sees the query string. The probe is a bare **GET** regardless of the failed request's
  method — never a mutation — and its answer is read method-aware: 2xx *or 405* proves the route
  exists (405 is what the POST-only `/api/model/set` returns to GET,
  `hermes_cli/web_server.py:6584`) → the unknown profile is named; 404 → `absent_capability`; any
  other answer (401, 5xx, connection loss) reports the original error class and claims no
  disambiguation.
- **R10.** `session.list` gains a `MethodBaseline` entry in `talaria/domain/compat.py` before the
  first call site exists — the recorded rule "every gateway method Talaria names must be pinned in
  the compatibility baseline" (DECISIONS.md, 2026-08-05).
- **R11.** Every implementation unit's diff (U1–U7) receives an external code review from the
  operator's Codex reviewer session before its PR round closes (KTD10). A unit implemented *by*
  that external engine is instead reviewed here — the review always crosses the engine boundary in
  one direction or the other, and a review whose reply was not captured verbatim is treated as not
  having run. A review that returns findings **gates**: the next implementation unit starts only
  after the findings are resolved in the reviewed unit's PR round. U8 produces no code diff; its
  deliverable (the evidence document) is reviewed in its own PR round on this side.
- **R12.** The release's operator-facing requirements (R1–R7) are validated **live** against a real
  gateway in the operator's testing terminal session, with the evidence recorded as a dated results
  document (the `docs/plans/2026-08-07-row6-live-evidence-results.md` convention) and recordings
  cited by digest (R29 discipline). A requirement with only headless evidence is not claimed met.

## Key Technical Decisions

- **KTD1 — Jump-key + hint line + focus styling, not modal answering.** The cards are already
  operable inline; the defect is *reachability* (a button card never receives focus at all,
  `talaria/ui/prompts.py:713-716`) and *legibility* (two focus styles in the whole package). A modal
  answering flow was rejected: prompts arrive mid-typing, so a keyboard-owning modal steals the
  composer at the worst moment, and the picker's selection protocol has no free-text concept for
  sudo/secret answers. The jump is deliberate, so it may move the caret even while the composer
  holds text — unlike mount-time auto-focus, which keeps its existing don't-steal guard
  (`focus_new`, `talaria/ui/app.py:1175`). Operator chose 2026-08-08.
- **KTD2 — Resumed history enters the domain as a dedicated pure transition, not synthesized
  events.** A `seed_history(state, messages, omitted, count)` transition appends committed
  `TranscriptEntry` values directly (the `record_replayed_submission` shape), rather than
  fabricating `GatewayEvent`s through `apply_frame`. Rationale: the reducers carry turn, prompt and
  segment bookkeeping that fake events would contaminate (`_HANDLERS`,
  `talaria/domain/state.py:1715+`); the projection serves **committed** content solely from
  `state.transcript` (`transcript_view`, `talaria/domain/projection.py:260-288` — its only other
  read is the provisional streaming tail from `state.streaming_text`); and the append-only invariant
  stays intact within a landed session by construction. Landing is a **barrier**: the seed is
  applied before the frame pump consumes any event that followed the reply — the transport resolves
  the RPC future (`talaria/transport/source.py:589`) and enqueues frames independently (`:601`)
  while the pump runs concurrently (`talaria/ui/app.py:988`), so without a barrier a live event can
  land before the history it follows. U6 pins the barrier with a reply-then-event back-to-back
  transport test.
- **KTD3 — The switcher reuses the startup landing path.** Switching is `session.resume(target)` →
  `_land_session` → `focus_session` + `seed_history`. One code path lands in a session whether at
  startup or mid-run, so U6's fix is exercised by two callers before release, and the switcher
  inherits the resume tests rather than needing its own landing semantics. Landing a **different**
  session begins a fresh transcript buffer before seeding: `focus_session` deliberately retains the
  transcript for its reconnect caller (pinned at `tests/domain/test_reconciliation.py:185`), and
  appending seeded history onto a retained foreign transcript is exactly the merged multi-session
  view the non-goals forbid. Reconnect (same session) keeps retention; the reconciliation pin
  learns the same-session/different-session distinction in U6. Landing records both identities: the
  runtime session id drives event correlation, the durable `session_key` drives picker identity
  (R6/R7).
- **KTD4 — Decline is the client's own escape answer per kind, bound to `escape` on a focused card
  control.** On the wire it is what the reference client's escape path sends — the empty field
  value for clarify/sudo/secret, the explicit `deny` choice for approvals (R3: an empty choice is
  *approved* by the gateway's consumer) — no new method, no protocol change. A decline follows the
  answer outcome discipline unchanged: cleared before sending, restored only on a definite
  `not_sent` (`talaria/domain/state.py:708` — hiding a card the gateway is still waiting on would
  remove the operator's only control over it), and latched into `flushed_prompt_ids` once
  consumption, expiry, or a confirmed interrupt makes it obsolete — the same latch expiry uses.
  `escape` in the composer is left unbound: with decline one key away, overloading `escape` as
  "jump" would make a double-press destructive.
- **KTD5 — The caret marker lives in the status region, in its own slot.** The caret gets a
  **dedicated** fixed-height, non-wrapping slot inside `StatusRegion`
  (`talaria/ui/status_region.py:32-53`) rather than reusing the existing `.status--marker` `Static`:
  every status tick overwrites the marker (`status_region.py:79`) and command failures render there
  (`talaria/status/runner.py:282`), so a shared slot either loses the caret word on the next tick or
  suppresses a failure the operator needs to see. A one-row slot inside the existing status row
  keeps R5's no-height-change constraint by construction. A composer border-colour indicator was
  rejected as permanently costing two rows; a placeholder glyph was rejected as unable to say
  *where else* the caret went. Operator chose 2026-08-08.
- **KTD6 — `/sessions` shadows the gateway's dispatchable command of the same name.** The name
  matches the intent, and a modal picker is strictly better than the gateway's text listing for the
  switching task. Cost accepted: the gateway's own `/sessions` output becomes unreachable from
  Talaria. Revisit if the gateway listing gains information the picker does not show. Operator chose
  2026-08-08; record in DECISIONS.md when the unit lands.
- **KTD7 — 404 disambiguation re-asks the bare path once, inside the admin client.** When a request
  that carried `?profile=` 404s, `AdminClient` issues one `GET` to the bare path, read method-aware
  (R9): 2xx or 405 means the route exists — "no such profile: <name>"; a second 404 means
  `absent_capability`; anything else reports the original error unchanged. The probe is **always** a
  GET — for the POST-only `/api/model/set` a bare POST would *be* the mutation, and its 405 answer
  to GET is precisely the route-exists proof. Rejected alternative: consulting the profile directory
  `load_profiles` already fetched (`talaria/ui/app.py:2166-2190`) — that data lives in app state the
  transport layer cannot see, and it can be stale; the re-ask is authoritative and costs one cheap
  GET on an error path only.
- **KTD8 — F4's decline rides the confirmed-interrupt reply, and sweeps per kind.** Outstanding
  prompts are declined only after `session.interrupt` confirms, preserving the rule that a lost
  outcome changes nothing. The prompts belong to the turn that just died; releasing the gateway's
  blocking wait beats letting it expire. The sweep's algorithm is per kind: outstanding approvals
  resolve with one `approval.respond {all: true, choice: "deny"}` (the existing `DENY_ALL_CHOICE`
  mechanism — individual answers are refused while multiple uncorrelated approvals are outstanding,
  `talaria/domain/state.py:559`, `:582`); clarify/sudo/secret each get their kind's empty answer;
  `terminal_read` needs nothing. Ids the sweep resolves latch; answers already in flight settle
  through their own outcomes first. Operator chose 2026-08-08.
- **KTD9 — The jump key is `F1`, with a recorded fallback.** F2–F10 are bound
  (`talaria/ui/app.py:651-670`). F1 is the remaining conventional choice; some terminal/OS
  configurations intercept it, so U1's verification drives it in tmux and one real emulator — the
  drive is written into U1's executable unit, not left as an afterthought — and if F1 proves
  unreachable the fallback is `ctrl+space`, recorded in DECISIONS.md with the measurement that
  forced it.
- **KTD10 — External code review rides the operator's live Codex reviewer session, in one
  direction per unit.** The operator runs a dedicated Codex reviewer session for this release,
  managed by their terminal-workspace tool and driven directly over its control CLI (send the
  review request into the pane, wait for the agent to settle, read the reply back). Default
  posture: each unit is implemented here and its diff is reviewed there before the unit's PR round
  closes. Inverted posture, available per unit where the work is mechanical and test-gated: Codex
  implements and the review happens here. Either way the review crosses the engine boundary
  exactly once, the pane's reply is captured verbatim as the review's evidence, and findings land
  in the unit's PR round like any reviewer's. A review that returns findings **gates** the next
  implementation unit until they are resolved (R11), and the execution spec wires each
  implementation unit to depend on its predecessor's review unit so the gate is enforced by the
  graph, not by convention. Requested by the operator 2026-08-08, direct-session
  driving confirmed as the intended mechanism the same day. The session's machine-local identity
  is recorded in the saga state, not in this public document.
- **KTD11 — Live validation happens in the operator's testing terminal, twice.** Early: U6's
  prerequisite recording — a real `session.resume` against a live gateway to pin the `messages`
  element shape — is captured there before the mapping is written. Late: U8's acceptance drive
  validates R1–R7 live in the same terminal, producing the dated evidence document R12 requires.
  Requested by the operator 2026-08-08; the session itself is machine-local operational context and
  is deliberately not named in this public document.

## Implementation Units

The execution spec's dependency graph is authoritative. Its logical spine is U1 → U2 (reach before
decline) and U5 → U6 → U7 (harden, seed, switch); U4 is fully independent. Two orderings in the
spec are execution serialization, not logic: U3 runs after U1/U2 and U6 after U2/U3 because all of
them edit `talaria/ui/app.py` (concurrent-writer safety), and each implementation unit additionally
waits on its predecessor's review unit (KTD10's gate). U8 closes the release and depends
transitively on everything before it. Per KTD10, every implementation unit's verification includes
the cross-engine review (R11) in whichever direction the unit ran; U8 produces no diff and its
evidence document is reviewed on this side (R11).

### U1. Reach and see: the jump key, card hint lines, and focus styling

Make every prompt card's control reachable in one press and visibly focused when reached.

**Scope:** Bind F1 (KTD9) to an action that focuses the first unanswered card's control —
`PromptCard.focus_answer()` extended so button-backed cards focus their first `Button`, not only
`Input`-backed cards (`talaria/ui/prompts.py:713-716`). Add a hint line to `PromptCard.compose()`
naming the card's keys (answer / decline / deny all where present), following the picker's hint
convention. Add focus styling to `PromptCard` (card-level tint like `AgentRow.-interruptible:focus`,
`talaria/ui/agents.py:127-129`) so a focused control's card is legible at a glance. Scroll the
jumped-to control into view (reuse `reveal_actions`, `prompts.py:817-865`). Verification includes
driving F1 under tmux and one real terminal emulator (KTD9); if F1 is intercepted, the `ctrl+space`
fallback decision is recorded with the measurement.

**Out:** removing the scroll containers from the tab chain (a settled Textual default with its own
tradeoffs — follow-up if tabbing still misleads after the jump key exists); any change to mount-time
auto-focus.

**Test scenarios** (`tests/ui/test_prompts.py`, `tests/ui/test_focus_returns.py`):
- F1 from the composer focuses an approval card's first choice button in one press, with two agent
  rows mounted above it (the variable-tab-distance case).
- F1 while the composer holds text still jumps (a deliberate act may steal the caret), and the
  composer text survives untouched.
- F1 with no outstanding prompts is a no-op that leaves the caret where it was.
- The hint line is on the rendered screen (`screen_text`) for a choice card and an input card, and
  absent for the unanswerable deny-all card except its own keys.
- A focused card is visually distinct in `export_screenshot` (assert the style marker, following
  `test_prompts.py`'s screen-assertion convention).
- After answering via the jumped-to control, the caret returns to the composer (existing
  `CaretReleased` path, `tests/ui/test_focus_returns.py` convention).

### U2. Decline: escape on a card, and F4 finishing the job

Give the operator a way out of a prompt that is not "wait for it to expire".

**Scope:** `escape` while a card's control holds focus posts the decline — the kind's client-escape
answer (empty field value for clarify/sudo/secret, explicit `deny` for approvals; R3/KTD4) through
the existing `respond_live` path (`talaria/ui/app.py:1563-1621`) — following the unchanged
answer-outcome discipline: restore on definite `not_sent`, latch on every other known outcome
(KTD4). `interrupt_live`'s confirmed branch (`talaria/ui/app.py:1401-1427`, dispatched by
`action_interrupt` at `:1251-1256`) then declines every outstanding prompt of the focused session
by KTD8's per-kind sweep — one deny-all for approvals, empty answers for the rest, nothing for
`terminal_read`. Fold in the deny-all restore latch from the queue ("a deny-all that succeeds can
re-offer a control the gateway already resolved"): a deny-all that resolves the queue latches every
id it swept — the same mechanism, measured while this code is open.

**Out:** any new wire method; decline affordance for `terminal_read` (auto-answered, never renders
a card, `talaria/ui/prompts.py:130-132`).

**Test scenarios** (`tests/ui/test_prompts.py`, `tests/domain/test_prompt_registry.py`,
`tests/transport/test_bridges.py`):
- Escape on a focused sudo `Input` sends `sudo.respond` with an empty value, clears the card, and
  returns the caret to the composer; the transcript records the decline.
- Escape on a focused approval button sends `approval.respond` with the explicit `deny` choice —
  never an empty choice, whose wire value the gateway's consumer would treat as *approved*
  (`tools/approval.py:3320`); the exact wire value is asserted at the bridge.
- A declined prompt whose outcome is a definite `not_sent` **restores** — the existing pinned
  discipline (`tests/ui/test_prompts.py:739`); any other known outcome latches and the card stays
  cleared.
- F4 with a mixed set outstanding — two uncorrelated approvals plus a sudo — resolves the approvals
  with one deny-all call and the sudo with an empty password; nothing is sent for `terminal_read`.
- F4 with one prompt outstanding: interrupt confirms, then the prompt is declined and the card
  clears; the next submission does not queue behind a dead card.
- F4 whose interrupt outcome is lost declines nothing and says so (existing unknown-outcome text).
- Deny-all sweeping three approvals latches all three ids; an in-flight single answer coming back
  `not_sent` afterwards restores nothing.

### U3. The caret marker in the status region

Say where the caret is whenever it is not in the composer, at zero layout risk.

**Scope:** Watch focus changes at the app level and write a short location word into a **new
dedicated caret slot** in `StatusRegion` (`talaria/ui/status_region.py:32-53`) — fixed-height,
non-wrapping, empty when the composer holds the caret, `caret: prompts` / `caret: transcript` /
`caret: agents` otherwise. The existing `.status--marker` `Static` is not reused: every status tick
overwrites it (`status_region.py:79`) and command failures render there
(`talaria/status/runner.py:282`), so sharing it clobbers one message or the other (KTD5).
Height-invariant by construction (R5).

**Out:** focus styling for the scroll regions themselves (the marker names them; styling them is a
follow-up if the marker proves insufficient).

**Test scenarios** (`tests/ui/test_status_region.py`, `tests/ui/test_focus_returns.py`):
- Tab moving the caret into the transcript pane puts `caret: transcript` on the rendered screen;
  returning to the composer clears it.
- The F1 jump (U1) shows `caret: prompts` while the card control is held.
- The caret word and a status failure message are both on the rendered screen at once; the next
  status tick leaves the caret word intact.
- The regions of the status row, `#body`, prompts, transcript, and composer are geometrically
  identical before/during/after every caret-slot state — including while a failure message is
  present (the R5 falsifier, asserted the way `composer.py:181-189`'s regression is; screen height
  alone cannot see body rows move).

### U4. `absent_capability` stops blaming the gateway for a typo

Only a 404 on the bare path is evidence of an absent capability.

**Scope:** In `talaria/transport/admin.py`, when a request that carried a `profile` parameter
returns 404, probe the bare path once with a bare **GET** (KTD7), read method-aware (R9): 2xx or
405 → `AdminError("unknown_profile", …)` naming the profile (405 is the POST-only
`/api/model/set`'s route-exists answer); 404 → `absent_capability` as today; anything else reports
the original error unchanged. Never probe with POST — a bare POST to `/api/model/set` would perform
the mutation. Applies to `model_options`, `model_info` and `set_default_model`
(`admin.py:498-572`) — `set_default_model` is the one live `?profile=` sender today
(`talaria/ui/app.py:2804`).

**Out:** any change to 401/400 handling; caching the disambiguation probe.

**Test scenarios** (`tests/transport/test_admin.py`):
- A gateway serving `/api/model/options` bare but 404ing `?profile=typo` yields `unknown_profile`
  naming `typo`, not `absent_capability` (extends the existing fixture's real-HTTP fall-through,
  `test_admin.py:446-457`).
- A gateway serving neither yields `absent_capability`, with exactly two requests on the wire.
- A profile-carrying **POST** to `/api/model/set` that 404s probes with a bare **GET**; the route's
  405 answer proves existence and yields `unknown_profile` — pinned with a fixture that routes by
  method (POST-only on that path), not one that answers every method alike.
- A probe answered 401 or 5xx reports the original 404's error class unchanged and claims no
  disambiguation.
- The probe is not issued when the failed request carried no profile parameter.

### U5. `focus_session` is made safe to call twice

Fix the two recorded defects before the switcher makes them live.

**Scope:** `talaria/domain/state.py:274-299`. Reset `withdrawn_approvals` (the P2). Replace the
queue's settle-don't-drop suggestion with **refuse-while-in-flight**: `focus_session` refuses a
switch while `answering` is non-empty — a late outcome resolving after a switch would mutate the
newly focused session's transcript (`talaria/ui/app.py:1623`), and the refusal window is one RPC
round-trip; U7 surfaces the refusal as a notice. `flushed_prompt_ids` is retained across switches
(dropping it is what resurrects closed prompts), with synthesized approval ids qualified by session:
the synthesis counter restarts per landing (`talaria/domain/state.py:1321`), so unqualified
retention would latch session B's first synthesized approval under session A's tombstone.
`prompt_view` (`talaria/domain/projection.py:346`) gains the session filter whose absence lets a
foreign prompt render. Correct the docstring: its named caller ("reconnect") is false today —
`_land_session` calls it on every startup (`talaria/ui/app.py:2393`).

**Out:** any UI; reconnect-path changes.

**Test scenarios** (`tests/domain/test_prompt_registry.py`, `tests/domain/test_transcript_state.py`):
- `focus_session` on a state with `withdrawn_approvals=3` returns 0 (the measured failing case from
  the queue, now pinned).
- `focus_session` while an answer RPC is in flight refuses the switch and changes nothing; after
  the outcome settles, the same switch succeeds.
- A prompt id latched before a switch cannot be restored after it.
- A synthesized approval id latched in session A does not block the identically numbered
  synthesized id arriving in session B.
- A prompt belonging to session A does not render while session B is focused.

### U6. `--resume` renders the conversation

Project the `session.resume` reply's history into the transcript the event stream feeds.

**Scope:** `seed_history` in `talaria/domain/state.py` (KTD2), fed by a **typed history decoder**
rather than a bare role→kind cast: live replies carry text rows (`role`/`text`/`row_id`), tool rows
(`role`/`name`/`context`), reasoning-only rows, and display metadata
(`tui_gateway/server.py:7110`, `:7119`, `:7157`); each maps to its `TranscriptKind`, and malformed
elements or unknown roles take the `unknown-event` posture — literal text, nothing dropped. The
decoder's element shapes are pinned in a **decoder contract test** with fixtures taken from
recorded live replies; `MethodBaseline` stays top-level by design (`talaria/domain/compat.py:379`),
so the nested shapes live in the decoder contract, not the baseline. `_land_session`
(`talaria/ui/app.py:2376-2395`) reads `messages`, `message_count`, `messages_omitted`, retains
**both** the runtime session id and the durable `resumed`/`session_key` identity the reply carries
(`tui_gateway/methods_session.py:498`), and applies the seed after `focus_session` — behind KTD2's
landing barrier (seed before the pump consumes any event that followed the reply). Landing a
different session begins a fresh transcript buffer before seeding (KTD3); the reconnect caller's
retention pin (`tests/domain/test_reconciliation.py:185`) learns the same-session/different-session
distinction here. The stub fixture is internally inconsistent today (`message_count: 3` with
`messages: []`, `tests/transport/test_session_startup.py:71-82`) and gains real bodies. Three
recorded live `session.resume` replies already exist in the local recordings set and pin the
element shapes above; a fresh capture in the operator's testing terminal (KTD11) is needed only for
a shape the existing recordings lack — either way the fixtures are cited by digest (R29
discipline).

**Out:** rendering in-flight turn state from the reply's `inflight` field (follow-up; the reply
carries it but the projection question is separable); any pagination of long histories beyond what
the reply itself delivers.

**Test scenarios** (`tests/domain/test_transcript_state.py`, `tests/domain/test_projection.py`,
`tests/transport/test_session_startup.py`):
- `seed_history` with three messages produces three committed entries in order, before any live
  entry, and `transcript_view` serves them (committed content comes from `state.transcript`
  alone — pinned).
- The decoder maps a text row, a tool row (`name`/`context`), and a reasoning-only row to their
  kinds; a malformed element and an unknown role each land as `unknown-event` with their literal
  text preserved.
- `messages_omitted=True` arrives with an **empty** `messages` array and `message_count=7` — the
  gateway's omission shape (`tui_gateway/methods_session.py:494`) — and renders the withheld line
  naming seven; `messages_omitted=False` renders no such line; a hypothetical partial delivery
  names the difference.
- An empty `messages` array with `message_count=0` seeds nothing and notices nothing.
- The startup fixture gains real message bodies: `--resume` lands and the resumed lines are on the
  rendered screen (closing the gap the grounding found — no existing test asserts resumed history).
- A `session.resume` reply followed immediately by a live event seeds first: the event renders
  after the seeded history (the KTD2 barrier, asserted at the transport level).
- A reply whose stored identity differs from the runtime id lands, correlates events by the runtime
  id, and reports the stored id for picker identity (R6/R7).
- Landing a different session shows only the switched-to session's history; landing the same
  session on reconnect retains the transcript (the reconciliation pin, updated to distinguish the
  two).
- A live event arriving after the seed appends after the seeded entries; a duplicate of a seeded
  message is not deduplicated (append-only; the gateway owns history truth).

### U7. The session switcher

`/sessions` opens the picker; choosing a session switches through the landing path.

**Scope:** A `MethodBaseline` for `session.list` (R10) with its shape taken from the same live
recording session as U6 — a new read-only baseline entry changes the derived startup probe set
(`talaria/domain/compat.py:295`) and its pinned counts (`tests/transport/test_compat_baseline.py`),
which this unit updates deliberately. A `PickerSource` over the listing (flat, single-stage — the
`/profiles` shape) showing title/id/recency and marking the focused session; rows are identified by
the **stored** session id `session.list` returns (`tui_gateway/methods_session.py:197`), and the
highlight matches on the durable identity U6 retains (R7). A local `/sessions` command registered
ahead of catalogue dispatch (KTD6) — the closed `LocalAction` type
(`talaria/domain/commands.py:329`) and its routing tests (`tests/domain/test_commands.py`,
`tests/transport/test_commands.py`) extend to carry it. Selection dispatches
`session.resume(target)` → `_land_session` (KTD3) — no new landing code. A switch refused while an
answer is in flight (U5) surfaces as a notice, like the disconnected refusal. Dismiss restores the
composer caret via the existing dismiss-callback convention (`talaria/ui/app.py:2049-2073`).
Record KTD6's shadowing decision in DECISIONS.md in this unit's diff.

**Out:** creating or deleting sessions from the picker; a merged multi-session view (explicitly out
of Talaria's v0.1 identity boundaries and staying out); live restocking of an open picker (the
picker's recorded staleness posture stands — selection against a stale listing refuses on the epoch
check).

**Test scenarios** (`tests/ui/test_sessions.py` (new), `tests/domain/test_selection.py`,
`tests/transport/test_session_startup.py`):
- `/sessions` opens the dialog listing the stub's three sessions with the focused one marked by
  its stored id; arrows + enter dispatch `session.resume` with the chosen id.
- `/sessions` while an answer RPC is in flight refuses with a notice and sends nothing (U5's
  refusal, surfaced).
- The switched-to session's history renders (U6 through the second caller) and session A's prompts
  are gone from the screen (U5 through its first UI caller).
- Escape closes without a wire call and returns the caret to the composer.
- `/sessions` while disconnected refuses with a notice, not a crash.
- The catalogue's own `sessions` entry is shadowed: the local command wins and the shadowing is
  asserted, so a future catalogue change cannot silently re-route it.

### U8. The live acceptance run

Validate R1–R7 against a real gateway in the operator's testing terminal, and write the evidence.

**Scope:** With every prior unit merged: drive the F1 jump with agent rows on screen (R1), read the
hint lines and focus styling (R2), decline a sudo prompt with escape and watch the card clear (R3),
F4 a turn with a prompt outstanding and confirm the card clears and nothing queues behind it (R4),
walk the caret through transcript/prompts/composer and read the marker (R5), exit and `--resume`
into the same session and read the restored conversation (R6), then `/sessions` across two real
sessions and read the switched-to history (R7). Record with `talaria --record` where a recording is
the evidence; write the dated results document R12 names; run at least one leg under a real terminal
emulator, converting a platform-matrix row from declared to observed opportunistically. The drive's
**safety envelope**: throwaway sessions created for the run, never the operator's working sessions;
no real credential is ever typed — the sudo leg exists to *decline* (R3), and any answered leg uses
a canary command that grants nothing; recordings are redaction-checked before they are cited; the
operator is present for the whole drive; a failing leg stops the run rather than improvising; the
run closes what it opened. The checklist authored first names the expected observation per leg.

**Out:** re-grading any v0.1 verdict row (a v0.2 result that clears one is restated in both the
evidence table and the gate block with a `Clears:` backlink, per the handoff's rule — not silently).

**Test expectation: none — this unit produces recorded live evidence rather than suite tests;**
every scenario above is already pinned headlessly by U1–U7's suites. Its deliverable is the results
document plus recordings cited by digest.

## Risk Analysis & Mitigation

- **The `messages` element shape is thinly evidenced.** The baseline stops at the top level by
  design and the stub fixture is inconsistent. Mitigation: three recorded live replies already pin
  the observed shapes; U6 writes the decoder contract from them before the mapping is written, and
  a fresh capture in the operator's testing terminal fills any shape the recordings lack. This is
  also why U6 precedes U7.
- **F1 may be intercepted by the terminal.** KTD9 names the fallback and the measurement that
  triggers it; U1's verification drives the key in tmux and one real emulator (which doubles as
  progress on the platform-matrix queue item).
- **Approval decline semantics are settled, not open.** An empty approval choice is *approved* by
  the gateway's consumer (`tools/approval.py:3291`, `:3320`) — decline for approvals therefore
  sends the explicit `deny` the reference client's own escape sends. No live-check fallback exists
  to fall to; U2's bridge tests assert the exact wire value per kind.
- **Session-switch mid-turn.** Switching while a turn streams touches the same bookkeeping U5
  hardens; a switch while a prompt answer is in flight is refused outright (U5), and the switcher's
  tests pin the refusal. The known-stale posture (the dialog does not restock while open) is
  accepted and recorded.

## Scope Boundaries

**Deferred to follow-up work** (planned, not now):
- **Block-level markdown** — the operator's explicitly requested feature; it is a requirement
  change (R6 of the v0.1 requirements) with an open bounded-rendering question, and it gets its own
  `/brainstorm` and an ADR before any plan. Chosen for v0.2's release scope, deliberately not this
  plan.
- Removing the scroll containers from the tab chain; focus styling for scroll regions (U1/U3 name
  the follow-up trigger).
- Rendering `session.resume`'s `inflight` turn state (U6).
- The evidence-gap spine (Linux driving, real-emulator matrix) — not a spine here, but U1's and
  U6's verification runs should be driven under a real emulator where cheap, converting published
  limits into measurements opportunistically.

**Non-goals** (outside this work's identity):
- A merged view across sessions; session create/delete from the picker.
- Any Hermes product surface beyond the four blocking prompts.
- The PyPI name request (deferred by decision, 2026-08-08).
- Scrubbing the inherited environment (prohibited — R1's environment clause stays as recorded).
