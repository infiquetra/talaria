# Doc review — v0.2 answerability & session story plan

**Verdict: ready.** Every finding from all three review lanes is verified and fixed in place; none
remain open. The plan needed this review: it carried one P0 that would have shipped a wire-safety
defect, and eleven P1s.

## Review-result contract

- **Target:** `docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md`, and its
  execution spec `docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-spec.json`
  (workflow re-emitted after the spec amendment).
- **Reviewed revision:** working tree at commit `d391bcd` (plan, spec, and workflow are untracked
  new files; the codebase they cite is the committed tree).
- **Blocked:** no. All findings fixed; zero open at any priority.
- **Review lanes:** (1) this session's readiness-skeptic pass; (2) a read-only citation
  verification agent (25 plan citations checked, 21 verified, 4 drifts — all fixed); (3) an
  external full-document review by the operator's live Codex reviewer session (KTD10's mechanism),
  16 findings, reply captured verbatim.
- **External evidence:** the Codex reply is captured verbatim, cited by digest (sha256
  `d13d0f2b8b5164987f894af19236f5d4650bb73343152186e5ee0661db542741`) and held in the machine-local
  saga evidence — not committed, because it embeds machine-local absolute paths. An uncaptured
  review is treated as not having run (R11); this one is captured.
- **Verification of external findings:** every Codex claim was independently re-verified before
  adoption — 19 line-level claims across Talaria and the installed Hermes checkout confirmed by a
  read-only agent, and the P0's approval-consumer evidence read directly in this session. No
  finding was adopted on the reviewer's say-so.
- **Linked plan/saga:** the v0.2 answerability saga (`task-talaria-v0-2-answerability-and-session-story`).

## Findings (all fixed)

| Key | Priority | Finding | Resolution |
| --- | --- | --- | --- |
| D1 | P0 | An empty approval `choice` is treated as **approved** by the gateway's consumer (`tools/approval.py:3291`, `:3320` block only `None`/`deny`); the plan's empty-choice approval "decline" would authorize the command. | R3/KTD4/U2 rewritten: decline is per kind — explicit `deny` for approvals (the reference client's own escape, `useInputHandlers.ts:180`), empty field value for clarify/sudo/secret (`:119`, `:128`). The live-check fallback language is gone; the wire value is asserted per kind. |
| D2 | P1 | "A declined prompt never restores" contradicts the pinned discipline: a definite `not_sent` restores (`talaria/domain/state.py:708`, pinned at `tests/ui/test_prompts.py:739`); latching it would hide a card the gateway still waits on. | Decline now follows the unchanged answer-outcome discipline: restore on definite `not_sent`, latch on every other known outcome. |
| D3 | P1 | F4's "decline every prompt" had no batch algorithm; individual answers are refused with multiple uncorrelated approvals outstanding (`state.py:559`, `:582`), and R3's "any prompt" contradicted U2's `terminal_read` exclusion. | KTD8 gains the per-kind sweep: one `approval.respond {all: true, choice: "deny"}` for approvals, kind-empty answers for the rest, nothing for `terminal_read`; R3 scoped to attended prompts; mixed-kind test added. |
| D4 | P1 | Seeding history through a landing path whose `focus_session` retains the transcript (pinned, `tests/domain/test_reconciliation.py:185`) builds the merged multi-session view the non-goals forbid. | KTD3/U6: landing a different session begins a fresh transcript buffer before seeding; reconnect retention stays; the reconciliation pin learns the distinction. |
| D5 | P1 | In-flight answer bookkeeping cannot survive a switch in one global state: a late outcome mutates the newly focused transcript (`app.py:1623`), and per-landing synthesized ids (`state.py:1321`) collide under an unqualified retained latch. | R8/U5: a switch is refused while any answer RPC is in flight (U7 surfaces the notice); synthesized approval ids are session-qualified; tests pin both. |
| D6 | P1 | The picker had no durable identity: `session.list` returns stored ids (`methods_session.py:197`), landing stores the runtime id (`app.py:2379`), and the reply's `resumed`/`session_key` was discarded. | R6/R7/U6/U7: both identities retained — runtime id correlates events, stored id drives picker identity/highlight; a differing-identity reply is tested. |
| D7 | P1 | "History before any live event" had no ordering barrier: the transport resolves the RPC (`source.py:589`) and enqueues frames independently (`:601`) while the pump runs concurrently (`app.py:988`). | KTD2 defines the landing barrier (seed before the pump consumes post-reply events), pinned by a reply-then-event back-to-back transport test in U6. |
| D8 | P1 | "role → TranscriptKind" understates the history schema: live replies carry tool rows (`name`/`context`), reasoning-only rows, and display metadata (`server.py:7110`, `:7119`, `:7157`); `MethodBaseline` is top-level only by design (`compat.py:379`). | U6 specifies a typed history decoder with a decoder contract test pinned from recorded live replies (three exist); malformed/unknown → `unknown-event` posture, nothing dropped. |
| D9 | P1 | The caret marker reused a slot every status tick overwrites (`status_region.py:79`) and failures render into (`runner.py:282`); the screen-height falsifier cannot see body rows move. | KTD5/U3: a dedicated fixed-height caret slot; falsifier strengthened to region-geometry assertions including the failure-present case. |
| D10 | P1 | The bare-path probe was undefined for the POST-only `/api/model/set` (`web_server.py:6584`): a bare GET 405s (misread as generic error), and a bare POST would perform the mutation. | R9/KTD7/U4: the probe is always a bare GET, read method-aware — 2xx/405 = route exists → `unknown_profile`; 404 → `absent_capability`; else the original error unchanged. Routed-by-method fixture required. |
| D11 | P1 | Review units observed but did not gate: implementation units depended on predecessors' code, not their reviews, and U8 had no review at all. | R11/KTD10: findings gate the next unit; the spec wires U2←CR1, U3←CR2, U6←CR3, U7←CR6; R11 explicitly narrows to U1–U7 with U8's evidence document reviewed on this side. |
| D12 | P1 | Spec file scopes omitted required surfaces: the closed `LocalAction` type (`commands.py:329`) and its routing tests, the derived probe-set pins (`compat.py:295`, `test_compat_baseline.py`), DECISIONS.md for KTD6, and the reconciliation tests owning the switch policy. | U6/U7 file lists and scopes extended with all of them. |
| D13 | P2 | Plan prose ("U3 and U4 are independent") contradicted the spec's serialization, and KTD9's real-terminal F1 proof was absent from U1's executable prompt. | The dependency paragraph now names the spec graph as authoritative and explains the `app.py` serialization; U1's prompt and returns carry the tmux + real-emulator drive. |
| D14 | P2 | Two U6 claims contradicted reality: `transcript_view` also reads `state.streaming_text` (not "nothing but `state.transcript`"), and the gateway's omission path sends an **empty** `messages` array (`methods_session.py:494`), not a partial one. | KTD2 reworded (committed content from `state.transcript` alone); the omission scenario now matches the observed shape, with partial delivery as forward-compatibility. |
| D15 | P2 | U8 would force an agent to invent risky live-run setup (real credentials, no stop rule, no cleanup). | U8 carries the safety envelope: throwaway sessions, no real credentials ever typed, canary commands, redaction-checked recordings, operator present, stop-on-failure, cleanup. |
| D16 | P3 | `prompt_view` cited at `projection.py:321` (inside `subagent_view`); it starts at `:346`. | Both occurrences corrected. |
| D17 | P3 | The confirmed-interrupt branch attributed to `action_interrupt`; it lives in `interrupt_live` (`app.py:1401-1427`), dispatched from `:1251-1256`. | Attribution corrected in R4 and U2. |
| D18 | P3 | `load_profiles` cited as `app.py:2137-2190`; the function is `:2166-2190` (the range began inside `fetch_profiles`). | Corrected in KTD7. |
| D19 | P3 | "the existing R4/AE8 discipline" was a bare cross-release reference colliding with this plan's own R4. | Qualified as v0.1's R4/AE8. |

## Applied fixes

All nineteen findings were fixed in place across the plan (requirements R3, R4, R6–R9, R11; KTDs 2,
3, 4, 5, 7, 8, 9, 10; the dependency paragraph; units U1–U8; the risk section) and the execution
spec (U1–U8 prompts/returns, dependency rewiring, file-scope extensions). The spec re-validates
(15 units), the workflow was re-emitted from it, and the spend is unchanged at 282 ordinal — no
tier moved. The graph deepened from 9 to 12 waves because reviews now gate; the widest wave remains
3, at the concurrency cap.

## Residual risk

- The refuse-while-in-flight switch policy (D5) and the fresh-transcript landing policy (D4) are
  design decisions made under review evidence rather than operator interrogation. Both are the
  minimal options consistent with the plan's recorded non-goals and the codebase's pinned
  discipline, but the operator may prefer the heavier alternatives (per-session prompt state;
  per-session retained transcripts) — say so and the plan amends before `/work`.
- Hermes-side evidence was verified against the installed checkout (`91a545ab1`) and the pinned
  baseline commit (`7f4d15515`); a gateway upgrade between now and U2/U4/U6 landing could move
  those lines. The behaviors, not the line numbers, are the claims.
