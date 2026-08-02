---
title: Talaria v0.1 prototype requirements doc review
type: docs
status: complete
date: 2026-08-02
target: docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md
classification: requirements
reviewed_revision: working-tree-untracked
reviewed_head: f4408d09419cce8121ae0d3251335468bda11eeb
target_sha256: b02ad5cedaa4ee31e16e3f04d13f8c37ffcda47098333551601745fc1b00be2d
hermes_evidence_revision: 7f4d15515
gate_status: advisory
ready_for_plan: true
blocked_for_work: true
---

# Talaria v0.1 prototype requirements doc review

## Verdict

**Advisory-ready for `/plan`; not authorized for `/work`.**

The current requirements document can safely drive planning without an agent silently choosing the
remaining HOW decisions. It now separates its product and safety contract from ten mandatory planning
closure obligations, maps every requirement to verification evidence, and names the gates that must
stop implementation when evidence is missing.

No actionable P0, P1, P2, or P3 **document finding** remains in this advisory pass. One P1 review-
ceremony finding remains: the required independent panel gate was not mechanically completed. That
blocks `/work`, but it does not block `/plan`; the document says the same at
`docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md:17-19`.

## Remaining findings by priority

| Finding                                                                                                                                                                                                                  | Priority | Status                 | Evidence                                                                                          | Required disposition                                                                                                                                |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------: | ---------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| DR15. The receipt-verified panel gate is incomplete. Three original delegation rows exist, but only one left a completed final response; the available `saga_gate.py` has no delegation-mode `panel-independence` check. |       P1 | unresolved for `/work` | `docs/reviews/evidence/2026-08-02-talaria-requirements-panel-completion.json`; gate details below | Keep this verdict advisory. Before `/work`, run a mechanically verifiable independent panel or record an explicit operator override with rationale. |

## Applied fixes

| Finding                                                                                                                                            | Priority | Status | Applied fix                                                                                                                                                                                                                                                                                     |
| -------------------------------------------------------------------------------------------------------------------------------------------------- | -------: | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| DR1. Requirement-to-verification coverage was incomplete.                                                                                          |       P1 | fixed  | Added concrete flow and acceptance coverage plus the complete R1–R40 traceability table at `docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md:377-392`. The final structural receipt reports no missing or unknown requirement references.                                     |
| DR2. The framework-selection gate could pass without proving bounded rendering, terminal correctness, installability, or a viable fallback.        |       P1 | fixed  | Promoted measurable renderer, install, platform, lifecycle, and protocol criteria into R34–R40 and AE5, AE10, and AE16; made fallback assessment mandatory in PC8 (`:227-255`, `:322-326`, `:348-351`, `:372-375`, `:461-462`).                                                                 |
| DR3. The prompt inventory conflated confirmation UI with the gateway protocol and omitted `terminal.read`.                                         |       P1 | fixed  | Corrected the contract to one approval path plus four blocking bridges—clarification, secret, sudo, and terminal-read—and specified terminal-read correlation, expiry, redaction, and response-shape closure (`:126-140`, `:266-270`, `:311-316`, `:463-466`).                                  |
| DR4. The external status ABI was committed without forcing its schema and timing policy to close before implementation.                            |       P1 | fixed  | R19 now blocks the first consumer until the v1 contract is fixed, and PC2 enumerates every field and runtime policy `/plan` must settle (`:170-180`, `:442-445`).                                                                                                                               |
| DR5. Startup could select an arbitrary session with no in-product recovery.                                                                        |       P1 | fixed  | R2 and AE12 enumerate explicit target, stored resume, and new-session behavior; PC5 requires deterministic precedence without introducing the deferred switcher (`:110-112`, `:357-359`, `:453-454`).                                                                                           |
| DR6. Recorder parity had no defined equivalence relation and overclaimed the receive-only TypeScript boundary.                                     |       P1 | fixed  | R28 and AE6 limit cross-language parity to receive-only input; the Python send path has separate proof. PC4 now requires explicit normalization of observation fields while preserving sequence, direction, frames, redactions, and parse-error semantics (`:209-212`, `:328-332`, `:448-452`). |
| DR7. Replay alone was cited as proof that live transport changes nothing above the transport boundary.                                             |       P1 | fixed  | AE16 now compares controlled live and replay sources for identical domain and view-model transitions and requires separate live timing, backpressure, disconnect, and reconnect evidence (`:372-375`).                                                                                          |
| DR8. The known-incomplete reconciliation catalogue was described but did not stop normalization work.                                              |       P1 | fixed  | R37 makes completion at a pinned Hermes revision a prerequisite before the normalization layer is implemented (`:241-245`).                                                                                                                                                                     |
| DR9. Transcript content preservation, reflow, scroll anchoring, and bounded history were acceptance behavior without complete requirement mapping. |       P1 | fixed  | R6 and R38 carry the requirements; AE5 verifies them under resize and long-stream pressure (`:121-122`, `:246-249`, `:322-326`).                                                                                                                                                                |
| DR10. “Probe every required method” could be implemented by invoking mutating RPCs because the terminal gateway has no capability endpoint.        |       P1 | fixed  | R34, F1, AE7, and PC7 now separate side-effect-free startup checks from pinned source/schema and isolated acceptance evidence; mutating methods are explicitly not probes (`:227-233`, `:259-264`, `:334-337`, `:457-460`).                                                                     |
| DR11. Recorder create/write/flush/close failures and raw parser-error diagnostics were unspecified.                                                |       P1 | fixed  | R25–R26 and AE15 require visible storage failure, prohibit false success, and forbid raw payload fragments in logs or diagnostics (`:196-204`, `:368-370`).                                                                                                                                     |
| DR12. Attach-token handling protected recordings but not command-line arguments, shell history, process listings, rotation, or reconnect.          |       P1 | fixed  | R1, R9, AE3, and PC10 now cover acquisition and every named exposure surface while leaving the mechanism to `/plan` (`:107-109`, `:134-140`, `:311-316`, `:467-469`).                                                                                                                           |
| DR13. Command-dispatch and session-registry terminology overstated protocol shapes.                                                                |       P2 | fixed  | The document now names a deferred session-registry **surface**, six dispatch result shapes (`exec`, `plugin`, `send`, `skill`, `alias`, `prefill`), and bundle as a use of `send`, not a seventh shape (`:49-54`, `:185-192`, `:343-346`).                                                      |
| DR14. The source list inherited a stale exact count for methods called by the shipping client.                                                     |       P2 | fixed  | Retained the verified 130 gateway-method count and removed the stale 32-call claim; the requirements name only their pinned dependency subset (`:477-492`).                                                                                                                                     |

## Planning closure obligations

The unresolved choices are deliberate plan inputs, not permission for an implementer to choose defaults.
`/plan` must resolve all ten obligations at
`docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md:437-469`:

1. Composer widget and submit/newline bindings.
2. Status payload ABI and execution policy.
3. Minimal sub-agent projection and terminal-state precedence.
4. Recorder equivalence and format-version disposition.
5. Startup session precedence.
6. Callable gateway catalogue versus client-local controls.
7. Pinned Hermes baseline, corpus, thresholds, support matrix, and safe compatibility evidence.
8. A plausible Python presentation-layer fallback before the first-choice gate runs.
9. Terminal-read projection, serialization, timeout, and unavailable-view behavior.
10. Gateway credential acquisition, storage, rotation, reconnect, and non-exposure guarantees.

A plan that leaves any PC item unresolved has failed this review's handoff contract.

## Gate status

This is **not a gate-verified panel verdict**.

A read-only bounded attempt used an external `saga_gate.py` gate-check script (from a private
plugin repository; path scrubbed) against Hermes `state.db`:

- `receipt-presence` returned a pass and wrote
  `docs/reviews/evidence/2026-08-02-talaria-requirements-advisory-receipt-presence.json`.
  That query proves child-session rows exist, but it also includes a later continuation session and does
  not prove that the three original reviewer units completed.
- Delegation-mode `tier-compliance` returned an expected-model pass for the pinned reviewer model
  (alias scrubbed) and wrote
  `docs/reviews/evidence/2026-08-02-talaria-requirements-advisory-model-compliance.json`.
  The available delegation check compares model strings; it does not mechanically resolve the
  manifest's `heavy-reasoner` role class and `high` effort band.
- The script's `panel-independence` command accepts only kanban receipts. No delegation-mode check was
  available, so panel independence was not mechanically verified.
- The scoped completion receipt at
  `docs/reviews/evidence/2026-08-02-talaria-requirements-panel-completion.json` records three original
  child rows but only one completed final reviewer response.

Therefore the named checks do not combine into a passing readiness gate. No operator override is
recorded.

## Verification evidence

- `docs/reviews/evidence/2026-08-02-talaria-requirements-structure.json` passed against the final target:
  40 contiguous requirements, 7 flows, 16 acceptance examples, and 10 planning obligations; complete
  direct coverage and traceability; no unknown IDs, broken repo-relative references, tabs, or trailing
  whitespace; all protocol-precision checks true.
- Final target SHA-256:
  `b02ad5cedaa4ee31e16e3f04d13f8c37ffcda47098333551601745fc1b00be2d` (495 lines).
- Talaria evidence base: HEAD `f4408d09419cce8121ae0d3251335468bda11eeb`; the target and this review were
  untracked at review time.
- Hermes protocol evidence was read at installed revision `7f4d15515`: 130 gateway methods, 21
  `session.*` methods, one approval path plus four blocking bridges, and six command-dispatch result
  shapes.
- No credentials, live token values, deployment, installation, source implementation, or gateway
  mutation was used by this review.

## Review result contract

- Target: `docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md`
- Classification: requirements
- Readiness: advisory-ready for `/plan`
- Blocked for `/work`: yes
- Document findings: fourteen fixed, zero remaining
- Review-ceremony findings: one P1 remaining
- Override rationale: none
- Next step: `/plan`, preserving every R1–R40 mapping and closing PC1–PC10 before source work

## Scrub note

Private identifiers were generalized in this file and its evidence receipts before commit: a
private plugin-repository path, a reviewer-model alias, and local session identifiers. Recorded
hashes reference the pre-scrub originals, and the target hash above predates the host
reconciliation's RC1 edit; current hashes live in the reconciliation artifact.
