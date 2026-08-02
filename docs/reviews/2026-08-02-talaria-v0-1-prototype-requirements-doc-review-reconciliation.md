---
title: Talaria v0.1 prototype requirements — host reconciliation of the external doc review
type: docs
status: complete
date: 2026-08-02
target: docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md
classification: requirements
reviewed_revision: working-tree-untracked
reviewed_head: f4408d09419cce8121ae0d3251335468bda11eeb
external_review: docs/reviews/2026-08-02-talaria-v0-1-prototype-requirements-doc-review.md
target_sha256_before: b02ad5cedaa4ee31e16e3f04d13f8c37ffcda47098333551601745fc1b00be2d
target_sha256_after: b55ed53e25f5d6870ad51a9d671e97876ec03c4336e8261f21abc910c2d9a089
hermes_evidence_revision: 7f4d15515
ready_for_plan: true
blocked_for_work: true
---

# Host reconciliation of the external doc review

## Verdict

**The external review's edits are verified and adopted, with one wrong claim corrected in place.
The document remains ready for `/plan` and remains not authorized for `/work`.**

The external pass rewrote the requirements document from 33 to 40 requirements and added flows,
acceptance examples, traceability, and planning obligations. Every protocol claim those edits
introduced was re-verified against the running Hermes source at `7f4d15515` and held, except one
mischaracterization of a repository reference, fixed below.

One blocker remains before `/work`: the inherited review-ceremony finding DR15. A second finding —
private operational context in the external review artifacts, which must not reach this public
repository's history — was resolved by scrubbing those files in place before the first commit
(RC2).

## Applied fixes

| Fix | Priority | Status | What changed |
| --- | -------: | ------ | ------------ |
| RC1. The Sources section called the protocol reference's client-call count "stale." | P2 | fixed | `docs/analysis/hermes-gateway-protocol-surface.md` is pinned at Hermes `7f4d15515` (2026-08-01) — the same revision as the running Hermes and every evidence pin in this review — so its count of methods the shipping client calls is at-pin, not stale. Reworded to keep the external fix's real point: the count measures Hermes's client, and Talaria names its own pinned subset instead of inheriting it. |
| RC2. The external review artifact and its evidence receipts carried private operational context — two private repository references, a reviewer-model alias, and local session identifiers — into a public repository whose `docs/reviews/` is tracked. | P1 | fixed | Identifiers generalized in place with stable scrubbed tokens before the first commit; each edited file carries a scrub note recording that its receipt hashes reference the pre-scrub originals. The gate is advisory, so the lost hash-chain integrity is an accepted cost, evidenced by the before/after hashes in this artifact. |

## Adjudication of external findings

Every external finding was verified against the document, this repository, or Hermes source before
adoption, per the advisory-opinion contract. DR14's applied edit was correct in what it removed but
wrong in what it asserted; the assertion is corrected by RC1.

| Finding | Adjudication | Verification |
| ------- | ------------ | ------------ |
| DR1 requirement-to-verification coverage | keep | Structure receipt re-checked by hand: R1–R40, F1–F7, AE1–AE16, PC1–PC10 contiguous; traceability rows spot-checked against AE coverage lines. |
| DR2 measurable framework-gate criteria (R34–R40) | keep | Criteria match the gate recorded in `docs/engineering-journal/QUEUED.md` (bounded mounting, scroll anchoring, install, platform matrix) and add falsifiable thresholds deferred to `/plan`. |
| DR3 one approval path plus four blocking bridges, including terminal-read | keep | Hermes source uses this exact taxonomy: the bridge lifecycle comment at `tui_gateway/server.py:2983` names secret, sudo, clarify, and terminal.read as "all four blocking bridges"; `approval.respond`, `clarify.respond`, `secret.respond`, `sudo.respond`, and `terminal.read.respond` are all registered methods. Expiry semantics (per-bridge `.expire` keyed by `request_id`, late responds tolerated) verified at `tui_gateway/server.py:2981-2998`. |
| DR4 status ABI frozen before first consumer (R19, PC2) | keep | Internal consistency verified; no external source claim to check. |
| DR5 startup session precedence (R2, AE12, PC5) | keep | `session.create`, `session.resume`, and `session.most_recent` all exist in the 130 registered methods. |
| DR6 recorder equivalence limited to receive-only input (R28, PC4) | keep | The TypeScript recorder is receive-only; `docs/formats/frame-log.md` is the versioned authority; equivalence relation deferred to `/plan` correctly. |
| DR7 live-versus-replay equivalence evidence (AE16) | keep | Sound strengthening of R31; no source claim to check. |
| DR8 reconciliation catalogue completion gate (R37) | keep | Matches the P1 already recorded in `docs/engineering-journal/QUEUED.md` — the turn controller has been read only at its call surface. |
| DR9 transcript reflow and bounded history mapping (R6, R38, AE5) | keep | Mapping verified in the traceability table. |
| DR10 no mutating capability probes (R34, AE7, PC7) | keep | Verified: the 130 registered methods include no capability or discovery endpoint (the only "discover" hit is repository discovery on disk), so the requirement's premise holds at `7f4d15515`. |
| DR11 recorder failure surfacing (R25–R26, AE15) | keep | Consistent with the recording contract; no source claim to check. |
| DR12 attach-credential exposure surfaces (R1, R9, PC10) | keep | Consistent with ADR-0001, which records the authenticated, dashboard-backed attach path with source citations (ADR lines 84–85). |
| DR13 six dispatch result shapes; bundle uses `send` (R24, AE9) | keep | Verified at `tui_gateway/methods_tools.py`: the handler's result types are exactly `alias`, `exec`, `plugin`, `prefill`, `send`, `skill`; bundles return `type: send`; the source comment states UIs render `display`, never the model-facing `message`. Official-client-local catalogue entries verified at `tui_gateway/server.py:11514` (`/density`, `/logs`, `/mouse`, `/sessions`). |
| DR14 stale client-call count in Sources | keep, with correction | The edit rightly stopped the requirements from inheriting the 32-method count as Talaria's contract, but the reference is pinned at the same `7f4d15515` as this review, so "stale" was false. Corrected by RC1. |
| DR15 panel ceremony incomplete | keep | The completion receipt shows three dispatched review units with one completed final response. Remains P1, unresolved for `/work`; advisory for `/plan`. |

Additional protocol spot-checks that held: `terminal.read.request` carries optional `start` and
`count`, and the tool contract declares the response as `{total_lines, start, end, viewport_rows,
cursor_row, text}` (`tools/read_terminal_tool.py:30,64`; request built at
`tui_gateway/server.py:5523-5528`).

## Remaining findings by priority

| Finding | Priority | Status | Required disposition |
| ------- | -------: | ------ | -------------------- |
| DR15 (inherited). The independent review panel was not mechanically completed: three units dispatched, one completed final response. | P1 | unresolved for `/work` | Before `/work`, complete a mechanically verifiable panel or record an explicit operator override with rationale. Does not block `/plan`. |

## Residual risk

The panel-independence property has no mechanical verifier in the available tooling, so DR15 can
only be closed by a completed re-run or an operator override — this review cannot close it.

The 32-method client-call figure was re-verified only by its revision pin matching the running
Hermes; the per-method list was spot-checked, not recounted call site by call site.

## Review result contract

- Target: `docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md`
- Classification: requirements
- Readiness: ready for `/plan`
- Blocked for `/work`: yes — DR15 (panel ceremony)
- External findings: fourteen kept (one with correction), one kept-unresolved
- Applied fixes: RC1, RC2
- Review artifact: this file, alongside the external review it reconciles
- Override rationale: none
- Next step: `/plan`, closing PC1–PC10
