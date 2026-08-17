# Doc review — the Talaria v0.4 fleet-turn plan (four-engine panel)

**Verdict: ready, after amendments.** Four independent engines reviewed the plan concurrently in
this working tree; their merged findings produced two blocking decisions (both ruled by the
operator, 2026-08-17), a requirements revision, and a mechanical fix set — all applied. The spec
and workflow were regenerated and revalidated. This artifact is the panel's durable record; it
supersedes the two single-engine artifacts that briefly occupied this path during the run.

## Panel

| Engine | Model / tier | Invocation | Verdict as returned |
| --- | --- | --- | --- |
| agy (Antigravity) | gemini-3.7-flash / high | adversarial review brief | CLEAR (1 P1, 2 P2) |
| codex | gpt-5.6-sol / max | `$saga:doc-review` | BLOCKED (1 P0, 8 P1, 2 P2; 6 safe fixes applied in place) |
| opencode | deepseek-v4-pro / max | adversarial review brief | BLOCKED (1 P0, 3 P1, 6 P2) |
| cursor | Kimi K3 Max | `/doc-review` | ready (7 fixes applied in place; 19/19 Hermes-side citations verified at `7095e23eb`, 20/24 Talaria-side) |

**Target:** `docs/plans/2026-08-16-talaria-v0-4-fleet-turn-plan.md`, with
`docs/plans/2026-08-16-talaria-v0-4-fleet-turn-spec.json` and the emitted
`docs/plans/2026-08-16-talaria-v0-4-fleet-turn.workflow.js`; origin requirements
`docs/brainstorms/2026-08-16-talaria-v0-4-fleet-turn-requirements.md`. Reviewed in the working
tree on main at `344f9c2` (all three plan artifacts untracked at review time). Codex and cursor
edited the plan mid-panel; every applied edit was independently re-verified at source by the
orchestrating session before acceptance.

## The two blocking findings and their rulings

**B1 — approval `request_id` (codex D1, P0; ruled: send when observed).** The plan's first draft
deliberately omitted the approval `request_id`. The gateway removes queue heads on timeout and
interrupt without emitting anything — the hazard `talaria/domain/state.py:959`'s
uncorrelated-approval refusal documents — and the running revision (`tools/approval.py:2593`)
synthesizes an id for every approval and aims exactly when it is supplied. Unsent, a closable
wrong-command-authorization window stays open. Ruling: the answer carries the observed id;
head-of-queue presentation, the uncorrelated refusal, and deny-all stand. R18 and KTD9 amended.

**B2 — unobserved-kind foreign items (opencode P0, codex D2, agy P1; ruled: guarded navigate plus
latched failure).** A foreign session exposes only a flattened `waiting`; activation hydrates only
approvals and clarifications (`tui_gateway/server.py:8708`). An unknown-kind item could therefore
trigger a confirmed, destructive attach and then resolve to nothing — a silent dead end
contradicting R17/AE11/OP3. Ruling: the item stays queued; the attach confirm names the
unknown-kind risk; non-hydration latches a visible resolved-failed on the row and the item (the
terminal-read settle precedent generalized). AE11 and R14 amended; KTD2/KTD8/U6/U7 updated.

## Applied fixes

In-place during the panel, audited and kept: codex S1–S6 (named-session guard correction — the
guard already verifies the named session, no relaxation existed to need; source-and-age plus
redaction/defang canaries across picker and seam surfaces; the U1 feature-inventory leg per
ADR-0003; U6 serialized after U4; frame-log contract in U2's scope; U9's reachable-endpoint
prerequisite) and cursor's seven (R19 → U6+U7 in the traceability table; U9's dependency line
matched to the spec graph; the connection-drop stale-since row test; `QUEUED.md` in U4's files;
KTD13's citation corrected to `projection.py:586`; KTD6 re-grounded on the reader's verified
unknown-key tolerance; the spec-graph-authority sentence).

Post-panel, from the merged findings (operator-approved as a set): KTD5 rewritten — endpoints
have one source, token-only per-profile entries, byte-identical dedupe or loud refusal, honest
document-level TOML failure semantics (codex D3, D4); KTD6 rewritten — multi-connection logs
write header version 2 so a version-1-only reader rejects rather than silently merging session
ids across gateways, single-connection logs stay version-1 byte-identical (codex D5);
`session.list` re-list cadence, epoch-paired (codex D6); machine-readable CR verdicts with the
/work driver halting on non-clean (codex D7); U9's operator-attention terminal state — workflow
completion never reads as release-ready (codex D8); spec and workflow regenerated and revalidated
(codex D9); protected-row cap semantics with visible truncation and cap/cap+1/protected-overflow
tests (codex D10); U1 preflight — operator-supplied workspace identity or halt before the first
mutation (codex D11); KTD13 owned by U6 with a `pending_prompts` pin test (opencode); the KTD12
"waiting ≥ observed span" render rule and test (opencode); the scripted activate-hydration
headless test (opencode); the `status-line.md` focused-scope qualifier and the `frame-log.md`
unknown-key statement (cursor).

Requirements revision (same day): R18, AE11, R14 amended as above; the dependency note's
`session_id` field name corrected (cursor).

## Residual risk

- The topology evidence behind PC1 remains scratchpad-grade until U1 formalizes the pinned read
  and re-verifies live; U1 is first in the graph and now halts without its workspace preflight.
- The installed Hermes checkout sat one commit ahead of and one behind its origin default branch
  at review time (codex); the pin is runtime evidence, not upstream-current evidence — U1 records
  the revision it actually verifies against.
- Cursor's rubric engine had no plan-stage rubric and its engine-offer helper returned no
  reviewer choices; its pass was citation-verification-weighted, which the other three engines'
  requirement-mapping passes complement.
