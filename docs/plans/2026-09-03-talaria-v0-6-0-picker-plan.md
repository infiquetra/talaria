---
title: Talaria v0.6.0 tier-T3 plan — single-Enter slash picker dispatch (#121)
type: fix
status: active
date: 2026-09-03
origin: infiquetra/talaria#121 (requirements ledger on infiquetra/talaria#118, finding F10)
backend: team-execution
---

# Talaria v0.6.0 tier-T3 plan — single-Enter slash picker dispatch (#121)

Summary: make Enter on a selected slash-command palette item dispatch it exactly once through the same resolve path as typed input, with argument and confirmation rules byte-for-byte authoritative and unchanged.

## Run placement

Tier T3 (with #122 + #125), review wave R3 (one frozen target, one Saga Code Review for all three). Base `d83a45670` (origin integration head, post-R1 merges, verified 2026-09-03; dispatch refs re-anchored post-#119: `SLASH_EXEC_METHOD` talaria/domain/commands.py:122, `resolve_command` :681, `decode_slash_exec` :837, `ParsedCommand` :610, `parse_command_line` :617). R2/R3 boundary: re-integrate the R2 merge at dispatch; same-wave `talaria/ui/app.py` sharing with #122 plus R2's #120/#124 is sequenced by the architect. Content gate from #119 (honest catalog truth shapes what the picker dispatches — evidence recorded). Merge gate into #127. No implementation starts before Jeff approves this tier.

## Problem Frame

The slash palette knows what is selected (`PaletteRegion.selected_entry`, talaria/ui/palette.py:339, via `filtered_entries` :331 and `selected_index` :335) while focus stays in the composer (:264) — but selection does not dispatch. Operators must transfer the selection to the composer and press Enter again, a second step the requirement removes. The dispatch machinery itself is settled post-#119 (`resolve_command` :681 → `GatewayInvocation` :647 / `UnsupportedInvocation` :671, over `slash.exec` :122 with fallback semantics); only the palette-to-dispatch wiring is missing.

## Requirements

**R1:** Enter on a selected slash-command picker item executes it exactly once.

**R2:** partial typing behavior unchanged; existing argument and confirmation rules remain authoritative (including the indexed confirmation resend pattern at talaria/domain/commands.py:443-457).

## Key Technical Decisions

**KTD1:** palette dispatch calls the same resolve path as typed input — no parallel dispatcher.

`resolve_command` is the single funnel for text → invocation. Wiring the selected entry through it (rather than a palette-specific dispatch shortcut) makes single-dispatch provable in one place and keeps #119's honest-unavailable semantics applying identically to picked and typed commands. Rejected alternative: a palette-owned dispatch — a second funnel that could drift from the authoritative rules.

**KTD2:** argument and confirmation rules are read-only context for this unit.

The confirmation resend shape (:443-457) and `ParsedCommand`/`parse_command_line` (:610-617) are constraints the wiring must satisfy, not code to touch. Any rule change discovered mid-unit is a finding for the architect, never a drive-by edit — R2 forbids it.

**KTD3:** Enter idempotency is structural: selection is consumed once.

Key repeat and double-Enter must not double-dispatch. Consuming the selection (or guarding re-entry on the in-flight invocation) at the wiring point covers repeat, bounce, and impatient second presses with one mechanism rather than three debounces.

## Implementation Units

### U1. Wire selected-entry Enter to single dispatch.

Lane: worker-6. On Enter with an active slash selection, dispatch `selected_entry` (talaria/ui/palette.py:339) through `resolve_command` (talaria/domain/commands.py:681) exactly once per KTD1/KTD3; unselected-Enter and non-slash modes keep current behavior.

**Test scenarios:** selected-item Enter dispatches once (assert invocation count, not just outcome); unselected Enter unchanged; slash-filtered typing path unchanged (tests/ui/test_slash_palette.py).

**Failure modes:** null selection, stale selection (catalog changed since render), and unsupported entries resolve through the normal path — unsupported stays unsupported with its notice, never dispatched, never silently dropped.

### U2. Argument and confirmation safety proof.

Lane: worker-6. Prove R2: picked commands with required arguments still demand them, and confirmation-gated commands still gate (including the indexed resend shape :443-457) — identical outcomes for picked vs typed input across the matrix.

**Test scenarios:** picked-vs-typed parity matrix for args-required, confirmation-required, and plain commands (tests/ui/test_slash_palette.py, tests/domain/test_commands.py).

**Failure modes:** a picked command that skips a gate its typed twin enforces fails the unit — parity is exact, and any divergence is an architect finding, not a palette exception.

### U3. Repeat and double-Enter idempotency.

Lane: worker-6. Key-repeat, double-Enter, and Enter-during-in-flight dispatch each produce exactly one invocation per KTD3.

**Test scenarios:** simulated repeat/bounce/double-press sequences assert single invocation; in-flight Enter is consumed or queued per the existing invocation lifecycle, never forked (tests/ui/test_slash_palette.py).

**Failure modes:** a second Enter landing after completion starts a new intentional dispatch only through the normal fresh-selection path — idempotency covers repeats, not user-initiated re-runs.

## Scope Boundaries

In scope: F10, the palette-to-dispatch wiring, parity and idempotency proof, the tests above.

Deferred to follow-up work: nothing currently — any follow-up is re-derived live, not pre-listed.

Non-goals: typing-behavior changes, argument/confirmation-rule changes, picker redesign, parallel dispatch logic, deferred-list widening.

## Verification

```shell
uv run pytest tests/ui
```

## Route

Proposed `backend: team-execution` and wave-R3 single-PR destination — confirm-at-tier-approval by Jeff. Recommended next after approval: `/doc-review`, then `/work`. Saga tick by the controller at dispatch. KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval.
