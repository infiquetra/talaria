# Hermes reconciliation-rule catalogue

Status: `active`
Authority: `reference`
Source: [Hermes Agent](https://github.com/NousResearch/hermes-agent) at `7f4d15515` (2026-08-01)
Closes: the ADR-0003 P1 in [QUEUED.md](../engineering-journal/QUEUED.md) — "Read Hermes's turn
controller and complete the reconciliation-rule catalogue"

ADR-0003 decided that Talaria treats Hermes's terminal UI as **documentation of behavior, not a
source tree to translate**, and named the reconciliation rules as one of the three things worth
taking by name. It also recorded what was still missing: the 2026-08-02 read covered
`ui-tui/src/app/createGatewayEventHandler.ts` (1,419 lines) but only the *call surface* of
`ui-tui/src/app/turnController.ts` (1,092 lines). This document closes that gap. Both files are now
read in full at `7f4d15515`, and every rule found in either is listed below with a verdict and a
named test.

## Why this is worth a document rather than a code comment

ADR-0003 names the failure mode precisely, and it is the reason this exists as a checkable artifact:

> The inventory must be systematic rather than opportunistic, because the failure mode is silent — a
> missed rule produces a defect months later that Hermes fixed years earlier, and nothing in the
> codebase points at the omission.

A rule that is dropped on purpose and a rule that was never noticed look identical in a diff. So
every rule below carries an explicit verdict, and `tests/domain/test_reconciliation.py` parses this
file and fails if any rule names a test that does not exist. The catalogue cannot rot quietly.

## What the turn-controller read added

ADR-0003 measured the handler file at roughly 22% reconciliation and error-recovery content and left
open whether the controller it delegates to would recover part of the reuse argument. It does not,
and the reason is worth recording: **the controller's density is in streaming presentation, not in
protocol reconciliation.** Of its 1,092 lines, the large majority is segment assembly, tool-shelf
coalescing, reasoning pulse timers, notice TTL machinery, and markdown/diff de-duplication — all of
which are decisions about how Ink renders a transcript, and none of which transfer to a client that
renders plain text.

What it *did* add is a set of rules that are individually one to four lines each and individually
invisible: the interrupt latch and its transcript trace, the final-tail dedupe and its interim
exemption, the "never assign `rendered` during streaming" fix, the stable sub-agent identity and
ordering, the nullish-preserving partial-payload merge, and the update-only upsert. Nine of the
thirty-six rules below come only from the controller. The reuse argument does not recover; the
*catalogue* does.

Two findings came from cross-reading the gateway rather than the client, and both are gaps in the
shipping terminal UI rather than rules it encodes — RR-27 and RR-28 below.

## Reading the verdicts

| verdict | meaning |
| ------- | ------- |
| **re-encode** | The rule holds for Talaria as Hermes states it. One line plus a test. |
| **re-encode with a change** | The rule's *problem* is real, Talaria's answer differs. The change and its reason are in the rule's note. |
| **drop** | Deliberately not carried. Always because a Talaria requirement or scope boundary says so, never because it looked unimportant. The named test proves the drop is still deliberate. |

Every rule's test lives under `tests/domain/`. Line references are at `7f4d15515`; the shorthand
`handler` means `ui-tui/src/app/createGatewayEventHandler.ts` and `controller` means
`ui-tui/src/app/turnController.ts`.

## The catalogue

| Rule | Behaviour | Evidence at `7f4d15515` | Verdict | Named test |
| ---- | --------- | ----------------------- | ------- | ---------- |
| RR-01 | An event naming a session other than the focused one mutates nothing; `gateway.*` types are exempt because they describe the transport, not a conversation. | handler `:717-722` | re-encode | `test_another_sessions_event_mutates_nothing` |
| RR-02 | A sub-agent status outside the frozen seven-member enum falls back to a safe value, after lower-casing. | handler `:364-382` | re-encode | `test_unknown_subagent_status_falls_back_safely` |
| RR-03 | A terminal sub-agent status is never overwritten by a later live event. Hermes's own comment names the clobber: "a stale `subagent.start` / `spawn_requested` can clobber a terminal state from complete". | handler `:606-612` | re-encode | `test_a_terminal_row_survives_every_late_live_event` |
| RR-04 | `subagent.complete` is authoritative and *may* replace an earlier terminal status — RR-03 guards against stale starts, not against the completion itself. | handler `:1306-1316` | re-encode | `test_a_completion_may_still_overwrite_another_terminal_status` |
| RR-05 | A clarify prompt abandoned by a backend timeout is flushed into the transcript exactly once, from whichever of the two paths notices first. | handler `:399-426`, called at `:1122-1127` | re-encode | `test_an_abandoned_clarify_is_recorded_once_across_both_paths` |
| RR-06 | Sub-agent identity prefers the server-issued `subagent_id` and falls back to a composite key, so an older gateway produces a flat list rather than merged rows. | controller `:1013-1016` | re-encode | `test_subagent_identity_prefers_the_server_issued_id` |
| RR-07 | A late `subagent.complete`/`tool`/`progress`/`thinking` never creates a row — it updates or is dropped. Otherwise a finished child is resurrected after the turn ended. | controller `:1021-1027`; handler `:1247`, `:1266`, `:1287`, `:1301` | re-encode | `test_a_late_event_never_resurrects_a_child_whose_start_was_missed` |
| RR-08 | Streaming sub-agent events carry partial payloads, so a field the event omits keeps its prior value rather than being overwritten with null. | controller `:1046-1076` | re-encode | `test_a_partial_payload_never_clears_a_field_it_omits` |
| RR-09 | Sub-agent rows are ordered by spawn position (depth, index), not by arrival, or grandchildren shuffle relative to siblings under concurrency. Talaria appends the identity to make the comparison total, which Hermes's two-key sort is not. | controller `:1078-1083` | re-encode with a change | `test_rows_are_ordered_by_spawn_position_not_arrival` |
| RR-10 | Per-sub-agent detail lists skip a repeated tail entry and keep only the last N, so a child polling the same note cannot fill its own row. | handler `:355-362` | re-encode | `test_detail_lines_dedupe_and_stay_bounded` |
| RR-11 | After an interrupt, every subsequent write for that turn is suppressed. Hermes implements it as an `interrupted` latch checked at the top of eleven entry points. | controller `:114`, `:443`, `:528`, `:669`, `:690`, `:716`, `:767`, `:797`, `:818`, `:880`, `:901` | re-encode | `test_late_deltas_and_tool_events_after_cancelling_are_ignored` |
| RR-12 | An interrupt always leaves a transcript trace — partial text folded and marked when there is any, a bare note when there is not — "so the transcript always records that the turn was cancelled". | controller `:322-331` | re-encode | `test_cancelling_with_nothing_streamed_still_leaves_a_trace` |
| RR-13 | A `message.complete` arriving after an interrupt does not render. R4 additionally requires that it cannot overwrite the cancelled state, and Talaria keeps `turn` reporting `cancelled` rather than settling to `idle`. Usage still merges: token accounting is not a claim about the outcome. | handler `:1349-1360`; controller `:638`, `:659` | re-encode with a change | `test_a_late_completion_cannot_overwrite_the_cancelled_state` |
| RR-14 | Only a new `message.start` clears the cancelled state — cancellation is terminal for the turn it cancelled, not for the session. | controller `:989` | re-encode | `test_a_new_turn_clears_the_cancelled_state` |
| RR-15 | Streaming deltas are *appended*; `payload.rendered` is never read while streaming. It is an incremental Rich-ANSI fragment, so assigning it on each tick discards everything streamed so far — visible as overlapping coloured text and lost prose. | controller `:669-687` (the `#16391` fix) | re-encode | `test_rendered_is_never_read_during_streaming` |
| RR-16 | The turn's final text prefers raw `text` over `rendered`, because `rendered` is ANSI for terminals that cannot render markdown and passing it through garbles the transcript. | controller `:566-572` | re-encode | `test_final_text_prefers_raw_text_over_rendered_ansi` |
| RR-17 | The final message's text is stripped of the segments the transcript already shows, or every reply's opening renders twice. | controller `:81-93`, `:582` | re-encode | `test_final_message_does_not_repeat_a_sealed_interim_message` |
| RR-18 | Interim-sealed segments are exempt from that dedupe **unless** `response_previewed` says the final text is the same response already published provisionally. | controller `:576-582` | re-encode | `test_response_previewed_dedupes_against_the_sealed_interim` |
| RR-19 | `message.interim` text is authoritative — the streaming buffer is synced to it when the backend did not stream every token — and the sealed segment marks the dedupe boundary. | controller `:689-713` | re-encode | `test_an_interim_message_seals_the_stream_as_a_segment` |
| RR-20 | A whole reasoning block is adopted only when none was captured yet, so a gateway that sends both deltas and a final block does not duplicate it. | controller `:715-731`, `:407-412` | re-encode | `test_a_whole_reasoning_block_does_not_duplicate_streamed_reasoning` |
| RR-21 | Hermes bounds its reasoning buffer at 80,000 characters by discarding all but the last 60,000, and gates reasoning capture on a display setting. Talaria does neither: R6 puts reasoning *presentation* out of scope while requiring that its *content* is never dropped, and both behaviours drop content. The memory consequence is queued, not hidden. | controller `:716`, `:767`, `:778-780` | drop | `test_reasoning_is_committed_at_turn_end_and_never_truncated` |
| RR-22 | A `status.update` repeating the previous note is not recorded again — the gateway re-sends the same text on a timer. | handler `:815-821`; controller `:115` | re-encode | `test_a_repeated_status_note_is_recorded_once` |
| RR-23 | Usage is merged field-wise and never assigned wholesale, so a partial usage payload cannot zero a running total. | handler `:743`, `:1362-1364` | re-encode | `test_usage_is_merged_field_wise_and_never_zeroed_by_a_partial_payload` |
| RR-24 | Protocol noise announces itself once per connection rather than on every frame. Hermes also renders a 120-character preview of the offending payload; Talaria drops that half, because R5 forbids rendering untrusted raw bytes and a preview is exactly that. | handler `:1031-1044` | re-encode with a change | `test_protocol_noise_is_announced_once_per_connection` |
| RR-25 | A sub-agent event carrying empty text is dropped before the upsert, so a heartbeat cannot append a blank detail line. | handler `:1240-1244`, `:1279-1284` | re-encode | `test_an_empty_subagent_event_changes_nothing` |
| RR-26 | A prompt expiry clears the control **only** when its `request_id` matches, so a stale expiry cannot close a newer prompt. | handler `:1174-1182` | re-encode | `test_a_stale_expiry_cannot_close_a_different_prompt` |
| RR-27 | **Gap found in the shipping client.** The gateway emits `.expire` for all four blocking bridges — secret, sudo, clarify and terminal.read — but the terminal UI handles only `sudo.expire` and `secret.expire`. `clarify.expire` is covered indirectly by RR-05; `terminal.read.expire` is unhandled. Talaria routes all four through one registry (R8). | `tui_gateway/server.py:2981-2998` vs. handler `:1174-1182` | re-encode with a change | `test_every_bridge_expires_through_the_same_registry` |
| RR-28 | **Gap found in the protocol.** `approval.request` carries no `request_id` — the payload is `{description, command, choices, allow_permanent, smart_denied}` and `approval.respond` resolves by session key instead. R8 requires a keyed registry, so Talaria synthesizes a stable session-scoped key; only one approval can be outstanding per session on this protocol, so the key is stable rather than a guess. **Drifted at `7095e23eb` — see "Drift against the running install" below.** | handler `:1130-1147`; `tui_gateway/methods_prompt.py:886-920` | re-encode with a change | `test_approval_gets_a_synthesized_session_scoped_key` |
| RR-29 | A response whose `request_id` matches no outstanding prompt is refused before it reaches the socket. The gateway tolerates a late respond and answers `{"status": "expired"}`, so tolerance is not routing — R8's "a late response cannot be attached to a different request" can only be guaranteed in the registry that knows which ids are live. | `tui_gateway/server.py:10228-10239` | re-encode with a change | `test_a_late_respond_attaches_to_nothing` |
| RR-30 | Switching the focused session drops the previous session's live state — sub-agents, streaming buffers, turn phase — so session A cannot bleed into session B. **`prompts` is the deliberate exception** (U5, CR3 finding 1): the gateway keeps blocking on an outstanding bridge across a switch and never re-announces it, so clearing the registry would orphan the control forever. It is retained in the registry; `talaria/domain/projection.py`'s `prompt_view` session filter is what keeps it off the switched-to session's screen. | controller `:918-938` | re-encode with a change | `test_focusing_a_new_session_drops_the_previous_sessions_live_state` |
| RR-31 | A delta arriving with no `message.start` opens a turn, counts it, and says so in the transcript. Hermes drops these; R6 forbids dropping content and AE2 names "missing start" as a sequence that must land in a catalogued outcome. | handler `:751-754` | re-encode with a change | `test_a_delta_without_a_start_opens_a_visible_synthetic_turn` |
| RR-32 | Hermes archives each turn's spawn tree to disk via `spawn_tree.save` at turn end. Talaria does not: R17 says it reads sub-agent state and never authors it, and this is the concrete method that rule excludes. The consequence is that rows have no archive to move into, so they stay in view until the next turn starts. | controller `:640-652`; handler `:430-458` | drop | `test_the_compat_baseline_contains_no_subagent_authoring_method` |
| RR-33 | A duplicated frame is idempotent where the payload carries an identity and additive where it does not — a repeated `subagent.start` updates one row, a repeated `message.delta` is more text, which is correct because the gateway does not retransmit deltas. | controller `:1018-1027` (identity lookup); AE2 | re-encode | `test_a_duplicated_frame_lands_in_the_catalogued_outcome` |
| RR-34 | Reordered sub-agent events converge on the same rows, because identity (RR-06) plus terminal protection (RR-03) make the fan-out order-insensitive. | controller `:1018-1086`; AE2 | re-encode | `test_reordered_subagent_events_converge_on_the_same_rows` |
| RR-35 | A malformed element inside a list payload is skipped; the frame is still processed. Hermes writes this for todo rows — reject the item, keep the list. Rejecting the frame would lose the clarify question along with a bad choice. | controller `:48-76` | re-encode | `test_a_malformed_element_inside_a_list_payload_is_skipped_not_fatal` |
| RR-36 | Hermes runs a notice state machine — held while busy, latest-wins, key-matched clear, TTL clock started on visibility, session-boundary clear — and a matching set of presentation dedupes for inline diffs and tool shelves. v0.1 has no notice surface and no diff rendering, so all of it is dropped; the underlying text is still committed as plain transcript content (R6). | controller `:173-265`, `:454-509`, `:598-609`, `:934-936` | drop | `test_a_notification_lands_as_a_plain_system_line` |
| RR-37 | A `gateway.stderr` line is clipped to 120 characters so one runaway line cannot own the **one-row activity region**. Both Hermes sites are scoped to `gateway.stderr` alone, and `status.update` reaches `pushActivity` unclipped — so the bound belongs to the region's height, not to the text being gateway-authored. Talaria re-encodes it for its one surface of the same shape, a sub-agent row's `detail` line, and bounds the scrolling transcript separately and far more loosely. | handler `:880`, `:1015-1016`; unclipped at `:811-817` | re-encode with a change | `test_clip_detail_line_marks_the_cut` |
| RR-38 | Inline diff content is preserved as plain text with the gateway's terminal-printer chrome (`┊ review diff`) stripped. Hermes's markdown fence, segment anchoring, and dedupe against the final narration are presentation decisions v0.1 does not make. | controller `:477-509` | re-encode with a change | `test_inline_diff_content_is_preserved_as_plain_text` |

## Corrections to earlier documents in this repository

Two things read differently at the pin than the repository's existing notes say. Both are recorded
here rather than silently fixed, because the point of a pinned read is that the disagreement is
visible.

**The inbound event count is 45, not 44.** `docs/analysis/hermes-gateway-protocol-surface.md` heads
its table "Inbound: 44 event types". The `switch` in the handler carries 45 `case` labels at
`7f4d15515`, and the table's own eight rows list 45 entries — the headline number is off by one
against its own body. `talaria/domain/decode.py` takes the 45 from the switch.

**Three more event types exist that the shipping terminal UI never handles.**
`terminal.read.request` is the fourth blocking bridge (raised at
`tui_gateway/server.py:5523-5528`), and `clarify.expire` and `terminal.read.expire` come from the
same expiry path that produces the two `.expire` events Hermes does handle
(`tui_gateway/server.py:2989-2998` names all four). Talaria's known-event set is therefore 48. A
client that did not know them would surface every desktop-style terminal read as protocol noise.

## Drift against the running install (recorded 2026-08-17, v0.4 unit U1)

The catalogue's evidence column stays pinned at `7f4d15515`. The v0.4 pinned read (`7095e23eb`, the
running install's checkout) drifts one rule, and the drift has a wrinkle: at recording time the
machine's serving processes still executed an intermediate revision (`91a545ab1`) on which the
drift had not yet landed — full detail in
[2026-08-17-v0-4-topology-verification.md](2026-08-17-v0-4-topology-verification.md).

**RR-28 — the approval-keying gap is closed upstream at `7095e23eb`.** Every queued approval entry
now synthesizes a `request_id` at construction (`tools/approval.py:2596`, `uuid4` setdefault); the
`approval.request` event payload and the new `approval.pending` snapshot rows carry it; and
`approval.respond` accepts an optional `request_id` that resolves exactly the named entry,
falling back to the FIFO head when omitted (`tools/approval.py:2655-2662`). Two consequences for
Talaria, both taken up by v0.4's KTD9:

- Talaria's synthesized session-scoped key remains the fallback identity, but when a gateway id
  was observed, the answer sends it — the gateway removes queue heads on timeout and interrupt
  without emitting anything, so an aimed answer is strictly safer than a positional one.
- On a pre-drift gateway the `request_id` parameter is **accepted without error** — verified live on
  `91a545ab1`, where `approval.respond` carrying a bogus id returned `{"resolved": 0}` rather than a
  parameter error. That the **FIFO head pops regardless** is *source-derived*, not live-verified:
  the observation was made against a session with an empty approval queue, where an id-aware and an
  id-ignoring implementation both return zero. The source basis is unambiguous — `tools/approval.py`
  at `91a545ab1` contains **zero** occurrences of `request_id` (8 at `7095e23eb`), so the parameter
  cannot be read there. Sending only *observed* ids keeps this harmless either way — a pre-drift
  gateway never emits one to observe.

Two smaller corrections that surfaced in the same read: the per-session approval structure is a
queue, not a single slot, at `7f4d15515` too (`tools/approval.py:2169` at that pin) — RR-28's
"only one approval can be outstanding per session" was a description of the shipping TUI's
presentation, never a protocol guarantee — and `session.active_list`'s `waiting` status never
covered approvals at any examined revision (the status function reads only the blocking-bridge
registry), so a session blocked on an approval reports `working`.

## What this catalogue does not cover

Everything in the handler that is terminal knowledge rather than protocol knowledge — the OSC-11
background probe, the xterm.js/tmux `#000000` finding, the OSC-10 foreground tiebreaker, the
platform-specific last resort — is roughly 310 lines and is deliberately out of scope here.
ADR-0003 lists it as a separate takeaway ("the hard-won terminal knowledge"), to be checked against
what the selected presentation layer already solves before being re-encoded. That check belongs to
U5, not to the domain core.

Also out of scope: Hermes's framework repairs. The forced redraw scheduled 40 milliseconds after a
theme change (`handler:101`) and the deferred configuration fetch that avoids React's re-render
guard are fixes to Ink. Porting them would import the problem along with the fix.

## Refs

- [ADR-0003](../../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md)
  — the decision this catalogue discharges
- [Hermes gateway protocol surface](hermes-gateway-protocol-surface.md) — event and method inventory
- [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md) — unit U3, KTD8, R37
- `tests/domain/test_reconciliation.py` — the parser that keeps this file honest
