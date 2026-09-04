---
title: Talaria v0.6.0 tier-T3 plan — script-driven multirow status bar (#125)
type: feat
status: active
date: 2026-09-03
origin: infiquetra/talaria#125 (requirements ledger on infiquetra/talaria#118, finding F6, recorded decision E)
backend: team-execution
---

# Talaria v0.6.0 tier-T3 plan — script-driven multirow status bar (#125)

Summary: render a true-bottom multirow status bar from the existing `status.command` runner with versioned JSON payloads, safe colors, refresh, bounds, and fallback — one runner, script trusted as configured, #126's deny boundary intact.

## Run placement

Tier T3 (with #122 + #121), review wave R3 (one frozen target, one Saga Code Review for all three). Base `d83a45670` (origin integration head, post-R1 merges, verified 2026-09-03; refs re-anchored to it — notably the R1-hardened contract: deny boundary talaria/status/contract.py:83-102, `build_child_env` :172, payload version gate :249-261). R2/R3 boundary: re-integrate the R2 merge at dispatch; R3-internal order is diagnostics (#122) first, then this unit (it builds on the freed region). Start gates from #126 and #122 evidence. Merge gate into #127. No implementation starts before Jeff approves this tier. Decision E (script trusted as configured; filtering, versioned contract, timeouts/bounds, fallback, no-secret behavior preserved) is a settled input, never re-opened.

## Problem Frame

The runner produces one tick result at a time and the bottom bar renders fixed segments (`BottomStatusBarView`, talaria/ui/status_bar.py:44; `SegmentSpec` :62; `build_status_bar_view` :82), while the payload contract pins version 1 with a hard gate (`assert_frozen_shape`, talaria/status/contract.py:249-261: `document["version"] != 1` raises). Script-controlled multirow fields have no intake, no version evolution path, and no true-bottom placement — and any extension must preserve the R1 deny boundary sitting on the same `build_child_env` path (:172-224).

## Requirements

**R1:** true-bottom multirow rendering from `status.command` with script-controlled fields, formatting, and safe colors.

**R2:** versioned backward-compatible payload evolution: old scripts keep rendering.

**R3:** refresh, bounds, responsive fallback; script edits effective on next refresh without restart.

**R4:** no second runner; never execute user scripts during planning.

**R5:** configured script trusted (decision E) with filtering, JSON/versioned contract, timeouts/bounds, failure fallback, and no-secret behavior all holding.

## Key Technical Decisions

**KTD1:** version evolution accepts v1 and gates forward, never breaks old scripts.

`assert_frozen_shape` stays the shape guard; evolution adds versions the normalizer (`normalize_status_segments`, talaria/status/contract.py:312) understands while v1 documents keep rendering identically. Rejected alternative: flag-day payload replacement — it would strand every deployed status script.

**KTD2:** one runner — the tick path extends, nothing spawns beside it.

`StatusRunner`'s at-most-one-invocation overlap policy is the unit's concurrency story; multirow rendering consumes `StatusTickResult`s, it does not add a scheduler. A second runner would double the child-process and credential surface for zero new capability.

**KTD3:** trust-as-configured composes with deny-by-construction.

Decision E trusts the operator's script choice; R1's deny boundary (talaria/status/contract.py:83-102, enforced in `_maybe_forward` :203 on every rule path) constrains what the child receives regardless of script content. The two compose because they govern different layers (which script runs vs what the child inherits) — the plan changes neither boundary, only the rendering intake.

**KTD4:** the bar owns script rows; the region keeps marker, focus, and seams.

True-bottom placement goes to the bar surface (`BottomStatusBarView` :44); `StatusRegion` keeps its marker/focus/seam roles (talaria/ui/status_region.py:34-110) as #122 leaves them. Coexistence is by ownership, not by z-order negotiation.

## Implementation Units

### U1. Versioned JSON payload intake with v1 compatibility.

Lane: worker-6. Evolve the payload contract per KTD1: v1 documents render byte-identical, new versions add script-controlled fields through the normalizer; the frozen-shape guard keeps rejecting actually-unknown shapes loudly.

**Test scenarios:** v1 golden payloads unchanged; new-version fields normalize; unknown versions/shapes raise rather than render junk (tests/status/test_payload_schema.py, test_process_contract.py).

**Failure modes:** null documents, wrong-typed fields, and version jumps all fall back to the previous good render with a notice — never a blank bar, never a crash.

### U2. True-bottom multirow rendering with safe colors.

Lane: worker-6. Render intake rows through the bar surface per KTD4 with the existing truncation machinery (`_take_prefix`/`_take_suffix`/`_middle_ellipsis`, talaria/ui/status_bar.py:145-198) extended to rows; script colors pass the safe-color rules; responsive fallback narrows gracefully.

**Test scenarios:** multirow placement at true bottom; color safety (unsafe colors contained); narrow-terminal fallback (tests/ui/test_status_bar.py, test_status_region.py for coexistence).

**Failure modes:** oversized content truncates with ellipsis rather than overflowing; terminal resize mid-render reflows on the next tick, never tears.

### U3. Refresh, bounds, timeouts, and next-refresh pickup.

Lane: worker-6. Tick-driven refresh with the runner's timeouts/bounds; script edits take effect on the next refresh without restart; failure fallback preserves the last good render.

**Test scenarios:** edit-then-observe pickup without restart; timeout yields fallback, not hang; bound violations clip with notice (tests/status/test_runner.py, tests/ui/test_status_bar.py).

**Failure modes:** a crashing script, an empty emit, and a hung child each resolve to fallback-plus-notice; the runner's overlap policy is never bypassed to "catch up".

### U4. Trust-and-filtering composition proof.

Lane: worker-6. Prove R5 end to end: configured script runs as configured while the R1 deny set, versioned contract, timeouts/bounds, fallback, and no-secret behavior hold simultaneously — including the four synthetic keys denied when allowlisted.

**Test scenarios:** trust-path plus deny-path combined suite (tests/status/test_env.py, test_runner.py); no real credentials anywhere.

**Failure modes:** any composition break (trust works but filtering lapses, or vice versa) fails the unit — R5 is conjunctive, and a half-holding boundary ships nothing.

## Scope Boundaries

In scope: F6, decision E as specified, the surfaces and tests above.

Deferred to follow-up work: nothing currently — any follow-up is re-derived live, not pre-listed.

Non-goals: second runner, planning-time script execution, provenance-policy invention, weakening #126 filtering, bottom-bar ownership disputes with #122, deferred-list widening.

## Verification

```shell
uv run pytest tests/status tests/ui
```

## Route

Proposed `backend: team-execution` and wave-R3 single-PR destination — confirm-at-tier-approval by Jeff. Recommended next after approval: `/doc-review`, then `/work`. Saga tick by the controller at dispatch. KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval.
