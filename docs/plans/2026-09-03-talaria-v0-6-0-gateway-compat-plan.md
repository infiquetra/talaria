---
title: Talaria v0.6.0 tier-T1 plan — gateway command compatibility (#119)
type: fix
status: active
date: 2026-09-03
origin: infiquetra/talaria#119 (requirements ledger on infiquetra/talaria#118, findings F1–F2)
backend: team-execution
---

# Talaria v0.6.0 tier-T1 plan — gateway command compatibility (#119)

Summary: repair catalog drift and `/model` refusal against the live gateway using only supported routes, with truthful degraded reporting throughout and a conditional, approval-gated Hermes annex.

## Run placement

Tier T1 (with credential hardening), review wave R1 (one frozen target, one Saga Code Review for both units). Integration branch `work/talaria-v0-6-0-integration` at base `8d9747dac` (= origin/main, verified 2026-09-03). Per-child worktrees/branches are created at dispatch by the controller and recorded on #119; this plan records branch/base, it does not create them. Downstream content gate into #121; merge gate into #127. No implementation starts before Jeff approves this tier.

## Problem Frame

The gateway answers `commands.catalog` with a `commands` key the pinned baseline does not model, so drift detection reports a supported response as drift; and `/model` surfaces a LOCAL dispatch-fallback refusal (4018) whose root cause — gateway refusal vs local fallback — is unestablished. Both failures erode trust in exactly the surface operators read first.

## Requirements

**R1:** `commands.catalog` diagnostics model the gateway's supported response including the observed `commands` key, with no false drift and no hidden warnings.

**R2:** `/model` uses a supported existing model route or reports honest-unavailable; only `GET /api/model/options`, `POST /api/model/set` and admin variants ship; the 4018 path never invents a gateway method.

**R3:** any Hermes-side change carries explicit file/repo custody, independent review, live Talaria-Hermes validation, documented delta plus rollback, Jeff's prior approval, and no secrets exposure — or "annex not activated" is recorded.

## Key Technical Decisions

**KTD1:** the `commands` key's semantics are established from live gateway evidence before choosing alias-merge vs optional-key modeling.

The baseline pins `pairs`, `sub`, `canon`, `categories`, `skills`, `skill_count`, `warning` (talaria/domain/compat.py:163-178, evidence `tui_gateway/methods_tools.py:255-367`); `compare_shape` (talaria/domain/compat.py:524) reports anything else as `unexpected-key` drift. Modeling a key whose meaning is unestablished would repeat the invented-method failure this child exists to close, so U1's investigator finding gates the U2 shape choice.

**KTD2:** drift detection stays top-level-only.

`compare_shape`'s scope note (talaria/domain/compat.py:524-531) deliberately excludes nested structure: top-level drift breaks attach, deep drift degrades one panel. Rejected alternative: pinning row shapes in the baseline — row shape already lives in decoder contract tests (see `session.list` precedent, talaria/domain/compat.py:184-195).

**KTD3:** the 4018 gateway-vs-local question is owned by the investigator with a live reproduction.

The module docstring (talaria/domain/commands.py:78-94) already derives the mechanism — `command.dispatch` refuses most catalog entries with 4018 while Hermes's own client calls `slash.exec` first — but derivation is not root cause. Only a live reproduction against the running gateway settles it, and only supported routes may ship either way.

**KTD4:** the Hermes annex activates only on the architect's written determination plus Jeff approval; otherwise U4 records "annex not activated" with the negative evidence.

Operator authority and ADR-0001 (Talaria never owns Hermes core) leave no other reading. If activated, the change ships through Hermes's own process, never this packet; if rejected or absent, Talaria degrades visibly per R2.

## Implementation Units

### U1. Diagnose 4018 and the `commands` key against the live gateway.

Lane: investigator. Establish whether the observed 4018 is gateway refusal or local fallback with a live reproduction, and establish what the `commands` key carries (alias of `pairs` rows vs distinct shape) with revision-cited gateway evidence. Output is a diagnosis note recorded on #119, and it gates U2's shape choice per KTD1.

**Test expectation:** none -- diagnosis artifact, not behavior; existing suites must stay green (`uv run pytest tests/domain tests/transport`).

**Failure modes:** gateway unreachable during diagnosis parks U1 with the attempt evidence rather than substituting derivation for proof; a `commands` key whose meaning stays unestablished forces the conservative modeling choice (optional-key, listing still unavailable) with the gap named.

### U2. Decode the `commands`-key catalog shape truthfully.

Lane: worker-6. Extend `decode_catalog` (talaria/domain/commands.py:274), which today reads only `result.get("pairs")` and renders a `commands`-keyed reply unavailable, to model the U1-established shape; keep the never-raises R5 discipline and the unavailable-catalog path for genuinely odd listings.

**Test scenarios:** `commands`-keyed fixture decodes with no false drift (tests/domain/test_compat_coverage.py); genuinely odd listings still yield unavailable-with-reason (tests/domain/test_commands.py); slash filtering unchanged.

**Failure modes:** empty mapping, null rows, duplicated names, huge row counts, and wrong-typed `categories` must all decode without raising and without promoting junk to dispatchable.

### U3. Pin the drift baseline and the `/model` honest path.

Lane: worker-6. Update the `commands.catalog` baseline entry (talaria/domain/compat.py:163) for the U1-established shape (response_shape and/or optional_keys with evidence revision); wire `/model` to the supported decoders (`decode_provider_catalog`, talaria/domain/models_catalog.py:347; `decode_model_assignment_result`, talaria/domain/models_catalog.py:390) with honest-unavailable when no supported route serves.

**Test scenarios:** baseline plus fixture reports no drift (tests/transport/test_compat_baseline.py); provider/assignment decoders cover options/set bodies (tests/domain/test_models_catalog.py); 4018 path reports honest-unavailable, warnings visible.

**Failure modes:** a gateway that drops a pinned key reports missing-key drift rather than crashing; `/model` against an incapable gateway says unavailable instead of trying an invented method.

### U4. Hermes-annex determination artifact (conditional).

Lane: architect. If and only if U1–U3 establish that a Hermes-side change is required, name the authoritative Hermes-owned repo/config/runtime from live truth, exact scope, and whether child decomposition changes; Jeff approves before any touch, and the bounded implementation then carries custody, independent review, live-interaction proof, delta plus rollback, and no-secrets handling. Otherwise record "annex not activated" with the negative evidence on #119.

**Test expectation:** none -- decision artifact; if activated, the implementation's tests are named in the approved determination, never invented here.

**Failure modes:** determination pending Jeff ruling parks only the annex, never U1–U3 evidence.

## Scope Boundaries

In scope: F1–F2 truthfulness, the conditional annex determination, decoder/baseline/test changes listed above.

Deferred to follow-up work: a genuinely required Hermes-side implementation (activates only via U4 approval, ships through Hermes's own process).

Non-goals: hiding validation warnings, inventing gateway methods, broad compat redesign, nested-shape pinning, any deferred-list widening.

## Verification

```shell
uv run pytest tests/domain tests/transport
```

## Route

Proposed `backend: team-execution` (worker-6 + reviewer + investigator + tester coordinated by the controller) and wave-R1 single-PR destination — both confirm-at-tier-approval by Jeff, not decided here. Recommended next after approval: `/doc-review`, then `/work`. Saga tick is written by the controller at dispatch (no saga runner in this repo). KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval, not before.
