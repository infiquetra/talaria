# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate; the harness is prepared, but acceptance has not run because the integrated artifact does
not exist yet.

## Status

**NOT RUN — harness authoring only.** Every verdict remains `PENDING`. No row in this document claims
product behavior, terminal appearance, live Hermes behavior, or model availability.

The source checklist is the **Visual acceptance checklist** in
`docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`. The machine-readable owner registry is
`docs/acceptance/v0.5.0/checklist-items.json`. A passing row requires an immutable receipt matching
`docs/acceptance/v0.5.0/receipt.schema.json`, a raw terminal capture, a real-terminal screenshot, and
a completed redaction review.

## Candidate provenance

| Field | Value |
| --- | --- |
| Integration branch | `PENDING` |
| Full candidate commit | `PENDING` |
| Wheel filename | `PENDING` |
| Wheel Secure Hash Algorithm 256-bit (SHA-256) digest | `PENDING` |
| Installed version | `PENDING` |
| `talaria-t1` install receipt | `PENDING` |
| `talaria-t2` install receipt | `PENDING` |

The install probe must reject a source checkout, editable installation, or global executable. Both
testers must install the same candidate wheel digest into distinct fresh virtual environments and use
distinct scratch configuration directories through `TALARIA_CONFIG_DIR`.

## Live model route status

The primary route is **OpenCode Muse Spark 1.2 Contributor Free**. The only permitted fallback is
**Ollama GLM 5.3 Flash**, and only for primary unavailability, connection failure, model-not-found, or
bounded-test incompletion. Every live receipt records the requested route, observed route, route
status, fallback availability, and the exact fallback reason when applicable.

The coordinator's pre-flight found no Ollama model installed on this machine and no resolvable GLM
5.3 variant. That is an open operator decision, not resolved by this harness. If a live primary leg
needs fallback while that condition remains, the leg is `BLOCKED`. It does not substitute another
model and cannot receive a passing receipt. A successful primary leg may still pass while the unused
fallback is unavailable.

## Safety envelope

1. Every application drive uses the executable proven by that tester's install receipt.
2. Raw American National Standards Institute (ANSI) terminal bytes remain in tester scratch until
   credential and private-identifier review. Unsafe material is withheld, not committed.
3. Deterministic flows may use the shipped `talaria replay` against frame-log corpora. Every live leg
   uses a real Hermes-backed throwaway session.
4. No Computer Use, GUI automation, mocked acceptance, or simulated Talaria application satisfies a
   row.
5. A timeout, empty capture, missing screenshot, missing route, silent substitution, unavailable
   required fallback, hang, or blank terminal state fails or blocks the row visibly.
6. Any operator-reserved step stays `RESERVED`; it is never simulated or converted into a pass.

## Evidence matrix

Shared rows require independent receipts from both testers. A dash means the tester does not own that
row. Receipt, capture, and screenshot paths are added only after sanitization review.

| Item | Checklist item | Owner tester | `talaria-t1` verdict | `talaria-t2` verdict | Receipt / capture / screenshot | Observation |
| ---: | --- | --- | --- | --- | --- | --- |
| 1 | Installed artifact | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 2 | Live primary route | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 3 | Main hierarchy | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 4 | Refined Default | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 5 | Dark Green Terminal | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 6 | Neutral Dark | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 7 | Accessible High Contrast | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 8 | Preview cancellation | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 9 | Explicit save and precedence | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 10 | Theme fallback notice | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 11 | Visual Studio Code import | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 12 | All status segments | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 13 | Status configuration | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 14 | Status responsive sequence | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 15 | Status failure visibility | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 16 | Inspector dock and resize | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 17 | Inspector content and empty states | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 18 | Inspector responsive state | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 19 | Side-by-side diff | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 20 | Unified fallback | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 21 | Diff navigation and boundary | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 22 | Composer caret location | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 23 | Connection non-color states | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 24 | Agent and queue non-color states | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 25 | Transcript identity without color | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 26 | Reduced motion | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 27 | Stable unpinned scroll | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 28 | Stable pinned scroll | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 29 | Wide screenshot | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 30 | Narrow screenshot | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 31 | Malformed Visual Studio Code import | `talaria-t1` | `PENDING` | — | `PENDING` | `PENDING` |
| 32 | Session-only status toggle | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 33 | Dead gateway credential | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 34 | Killed session | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 35 | Restart-only configuration | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 36 | Cross-tester evidence | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |

## Terminal and route summary

| Tester | Terminal program | `TERM` value | Dimensions exercised | Session profile | Live route observed | Fallback reason | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `talaria-t1` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| `talaria-t2` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` |

## Honest halt record

Record any prerequisite or runtime halt here. State the observed cause first, then the affected item,
tester, receipt path, and next authority. Do not turn a missing prerequisite into a product defect or
improvise around it.

`PENDING`

## Final verdict

**PENDING.** Acceptance has not run.
