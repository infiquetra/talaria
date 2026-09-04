---
title: Talaria v0.6.0 tier-T3 plan — diagnostics into the inspector (#122)
type: feat
status: active
date: 2026-09-03
origin: infiquetra/talaria#122 (requirements ledger on infiquetra/talaria#118, finding F5)
backend: team-execution
---

# Talaria v0.6.0 tier-T3 plan — diagnostics into the inspector (#122)

Summary: move routine operational rows (roster, approval detail, HTTP runner, Kanban dispatcher) into the inspector while actionable errors stay visible and the bottom status bar is retained untouched for #125.

## Run placement

Tier T3 (with #125 + #121), review wave R3 (one frozen target, one Saga Code Review for all three). Base `d83a45670` (origin integration head, post-R1 merges, verified 2026-09-03; all file:line refs re-anchored to it — local checkout trails at `01c8ffdc8` and is not the reference). R2/R3 boundary: T2 branches (`work/120-keyboard`, `work/123-appearance`; no `work/124` branch exists yet) are pending review — this unit re-integrates the R2 merge at dispatch, and same-wave `talaria/ui/app.py` sharing with #121 plus R2's #120/#124 is sequenced by the architect. Downstream content gate into #125 (the freed region shapes the multirow bar); merge gate into #127. No implementation starts before Jeff approves this tier.

## Problem Frame

The status region renders everything every tick — routine rows, seam rows, and failure markers share one surface (`StatusRegion`, talaria/ui/status_region.py:34, with separate row/seam lists at :58-59 and accessors `row_texts` :78, `seam_texts` :82, `marker_text` :86). Routine operational state (roster sweeps, approval-detail polls at talaria/ui/app.py:4619, `http-runner`/`roster`/`approval-detail` seams at talaria/ui/app.py:4472-4475) drowns the failures operators must act on, while the inspector (`Inspector`, talaria/ui/inspector.py:117, with `InspectorTaskRow` :47 and `InspectorFileRow` :76 precedents) already owns the drill-down pattern with nowhere for these rows to live.

## Requirements

**R1:** roster, approval detail, HTTP runner, and Kanban dispatcher rows move into the inspector.

**R2:** actionable errors and failures remain visible on the main surface; chat flow stays clean.

**R3:** the useful bottom status bar is retained (untouched — it is #125's surface).

## Key Technical Decisions

**KTD1:** the split rule is operator-action-required, decided row by row in this plan's U1.

Anything that needs the operator to act (failure markers via `_status_failure_marker`, talaria/ui/status_region.py:167; configuration notices via `show_configuration_notice` :100) stays in the region; informational state moves. Rejected alternative: moving whole row classes by origin — origin does not predict actionability, and a blanket move would hide failures.

**KTD2:** moved rows reuse the inspector's existing row precedents.

`InspectorTaskRow`/`InspectorFileRow` already define the row contract (mount, focus/blur, refresh line). New rows follow that shape instead of redesigning the inspector — the inspector gains content, not architecture.

**KTD3:** the bottom bar is read-only context for this unit.

`BottomStatusBarView` (talaria/ui/status_bar.py:44) and the script pipeline are #125's surface. This unit may relocate what feeds the region but changes nothing about the bar; the R3-internal order (diagnostics first, bar second) is what makes that safe.

## Implementation Units

### U1. Row-by-row actionable-vs-routine classification.

Lane: architect (recorded in the unit base note; worker-6 implements the resulting placement). Name every region/seam row, mark each stay-or-move with the KTD1 rule and the operator action it does or does not require. Output gates U2.

**Test expectation:** none -- classification artifact; suites stay green.

**Failure modes:** a row whose actionability is ambiguous stays visible (safe direction) with the ambiguity named for reviewer confirmation.

### U2. Move routine rows into the inspector.

Lane: worker-6. Relocate the U1-named routine rows to inspector rows per KTD2; region keeps marker, focus, and actionable rows with layout intact (`max-height: 11`, talaria/ui/status_region.py:34-53).

**Test scenarios:** each named row reachable in the inspector; region no longer renders moved rows; marker/focus rows unchanged (tests/ui/test_inspector.py, test_status_region.py).

**Failure modes:** empty upstream data renders the inspector row as an honest empty state (never a zero that reads as data); a failed move leaves the row visible in the region rather than dropping it.

### U3. Chat-flow and visibility proof.

Lane: worker-6, validated by tester. Steady-state ticks render a clean chat flow; injected failures still surface visibly with the marker path.

**Test scenarios:** clean-flow snapshot (no routine rows in region output); failure injection reaches the marker and notice paths (tests/ui/test_status_region.py, test_live_wiring.py).

**Failure modes:** a failure arriving while its row data is mid-move must still reach the marker — the move never races the alert path.

## Scope Boundaries

In scope: F5, the four named rows, inspector row additions, the tests above.

Deferred to follow-up work: nothing currently — any follow-up is re-derived live, not pre-listed.

Non-goals: bottom-bar changes (belongs to #125), inspector redesign, hiding any actionable failure, deferred-list widening.

## Verification

```shell
uv run pytest tests/ui
```

## Route

Proposed `backend: team-execution` and wave-R3 single-PR destination — confirm-at-tier-approval by Jeff. Recommended next after approval: `/doc-review`, then `/work`. Saga tick by the controller at dispatch. KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval.
