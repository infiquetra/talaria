---
title: Talaria v0.6.0 tier-T2 plan — Herdr-safe keyboard controls (#120)
type: feat
status: active
date: 2026-09-03
origin: infiquetra/talaria#120 (requirements ledger on infiquetra/talaria#118, findings F3–F4, recorded decisions A–B)
backend: team-execution
---

# Talaria v0.6.0 tier-T2 plan — Herdr-safe keyboard controls (#120)

Summary: make the inspector toggle and turn-cancel usable inside nested Herdr via a configurable `Ctrl+O` default and a `Ctrl+S` cancel default with honest footer labeling, live-tested for flow-control capture with no silent substitution.

## Run placement

Tier T2 (with #123 + #124), review wave R2 (one frozen target, one Saga Code Review for all three). Base `8d9747dac` (= origin/main = integration head, verified 2026-09-03; R1 branches `work/119-gateway-compat`, `work/126-harden` exist unmerged and touch none of this unit's surfaces except via `talaria/cli.py`, which this unit does not use — re-anchor at dispatch regardless). Same-wave `talaria/ui/app.py` sharing with #124 is sequenced by the architect; T3 units re-integrate after the R2 merge. No implementation starts before Jeff approves this tier. Decisions A (`Ctrl+O` default, configurable) and B (`Ctrl+S` default, cancel-vs-quit label, XON/XOFF live test, no silent substitution) are settled inputs, never re-opened.

## Problem Frame

`Ctrl+B` is captured by Herdr before Talaria sees it, so the inspector toggle is unreachable when nested; and the `Ctrl+C` interrupt action reclaims Textual's system quit binding (talaria/ui/app.py:363-368, BINDINGS talaria/ui/app.py:1136-1167, incumbents `ctrl+b` :1138 and `ctrl+c` :1146), so a mislabeled footer risks quitting the client when the operator meant to cancel a turn. No keybinding configuration surface exists today — configurability must be built, not wired.

## Requirements

**R1:** inspector toggle defaults to `Ctrl+O`, user-configurable, usable nested in Herdr; no unrelated keyboard redesign.

**R2:** the `Ctrl+C` action is replaced by `Ctrl+S` as the configurable default, with footer labeling that distinguishes cancel-turn from quit-client.

**R3:** `Ctrl+S` is live-tested across supported terminal/Herdr contexts for XON/XOFF capture or apparent freezes, in a real session with non-destructive actions; an unsafe context escalates, never silently substitutes.

## Key Technical Decisions

**KTD1:** configurability arrives as a new config surface feeding BINDINGS construction.

BINDINGS is a class-level list (talaria/ui/app.py:1136) with no config input anywhere (no keybind settings in `talaria/config.py`). Hardcoded bindings cannot satisfy "user-configurable", so the plan adds the setting plus the single construction point that applies it. Rejected alternative: per-action runtime remapping scattered across handlers — same blast radius with no single source of truth.

**KTD2:** `Ctrl+S` becomes the interrupt primary; existing aliases stay intact.

The A4 precedent keeps non-function-key primaries with function-key aliases (talaria/ui/app.py:1139-1150, e.g. `f4` for interrupt). `f4` remains, `ctrl+c` leaves the interrupt action, and quit stays anchored on `ctrl+q` (talaria/ui/app.py:1137) so cancel-vs-quit always has both ends visible.

**KTD3:** the idle guard keeps its voice, and the footer names both behaviors.

`action_interrupt` (talaria/ui/app.py:2395) already no-ops with `NOTHING_TO_INTERRUPT` (:368) when no turn is in flight and refuses in replay — the footer must label `Ctrl+S` as cancel-turn and keep quit-client labeled separately, so the existing guard reads as designed behavior rather than a dead key.

**KTD4:** XON/XOFF safety cannot be unit-tested; it is a tester-owned live protocol.

A terminal driver capturing `Ctrl+S` swallows bytes before Talaria ever sees them, so no pytest can cover it — U4 is live validation evidence across the supported contexts, with the unsafe-context escalation path to the architect and Jeff defined before testing starts.

## Implementation Units

### U1. Configurable inspector chord with `Ctrl+O` default.

Lane: worker-6. Add the keybinding setting, apply it at BINDINGS construction, default `toggle_inspector` to `Ctrl+O`; verify reachability in nested Herdr with host and client sessions preserved.

**Test scenarios:** default resolves to `Ctrl+O`; user override takes effect; invalid values fall back to the default without crashing (tests/ui/test_inspector.py, plus a focused keybinding test module).

**Failure modes:** empty, null, unknown-key, and duplicate assignments fall back safely; the incumbent `Ctrl+B` name stays documented as the replaced default.

### U2. `Ctrl+S` cancel default with honest footer labeling.

Lane: worker-6. Move the interrupt primary to `Ctrl+S` per KTD2, update the one-row binding listing (talaria/ui/app.py:1070) and help footer so cancel-turn and quit-client are unmistakable; keep the idle/replay guards' behavior and voice.

**Test scenarios:** footer rows name both behaviors; idle press still yields the nothing-to-interrupt notice; replay still refuses (tests/ui focus/inspector suites).

**Failure modes:** replay mode, idle state, and composer-focus contexts must each show the correct label and take the guarded action — never quit on a cancel press.

### U3. Focus-context coverage including picker and dialogs.

Lane: worker-6. Prove the chord works across focus contexts (composer, transcript, model/profile picker per talaria/ui/picker.py, dialogs); the surface the live #120 body names.

**Test scenarios:** toggle fires from each focus context without disturbing picker selection or dialog state (tests/ui/test_picker.py, test_dialog.py, test_focus_* suites).

**Failure modes:** a context that swallows the chord is named explicitly and routed to the U4 escalation path, not papered over.

### U4. XON/XOFF live-validation protocol and evidence.

Lane: tester (worker-6 implements any resulting fallback labeling only). Exercise `Ctrl+S` across the supported terminal/Herdr contexts in a real running session with non-destructive actions only; record per-context pass/freeze evidence on #120. An unsafe context triggers architect documentation plus a replacement proposal to Jeff — never a silent substitution.

**Test expectation:** none in pytest -- live evidence artifact; automated suites stay green.

**Failure modes:** apparent freeze in any supported context parks that context's sign-off while independent lanes continue; the unit cannot close until every supported context passes or Jeff rules on the replacement.

## Scope Boundaries

In scope: F3–F4, decisions A–B as specified, the surfaces and tests above.

Deferred to follow-up work: a Jeff-ruled replacement chord if a context proves unsafe.

Non-goals: unrelated keyboard redesign, remapping outside inspector/cancel semantics, destructive validation actions, deferred-list widening.

## Verification

```shell
uv run pytest tests/ui
```

## Route

Proposed `backend: team-execution` and wave-R2 single-PR destination — confirm-at-tier-approval by Jeff. Recommended next after approval: `/doc-review`, then `/work`. Saga tick by the controller at dispatch. KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval.
