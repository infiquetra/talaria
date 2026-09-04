---
title: Talaria v0.6.0 tier-T1 plan — status-script credential and configuration hardening (#126)
type: fix
status: active
date: 2026-09-03
origin: infiquetra/talaria#126 (requirements ledger on infiquetra/talaria#118, safety repairs)
backend: team-execution
---

# Talaria v0.6.0 tier-T1 plan — status-script credential and configuration hardening (#126)

Summary: prove the runner's default-deny boundary with synthetic keys and close the scalar-allowlist iteration hole at the single construction site, with strictly conditional rechecks and no broad audit.

## Run placement

Tier T1 (with gateway compat), review wave R1 (one frozen target, one Saga Code Review for both units). Integration branch `work/talaria-v0-6-0-integration` at base `8d9747dac` (= origin/main, verified 2026-09-03). Per-child worktrees/branches are created at dispatch by the controller and recorded on #126; this plan records branch/base, it does not create them. Downstream start gate into #125 (this hardened runner is what #125 extends); merge gate into #127. No implementation starts before Jeff approves this tier.

## Problem Frame

The child environment boundary is architecturally total — `build_child_env` (talaria/status/contract.py:156) asserts `is_suspicious_key` on every candidate from every rule path, allowlist included — but it is proven by documentation rather than by test, and the single construction site iterates whatever the config returns: `_build_status_runner` (talaria/cli.py:302-308) runs `tuple(str(name) for name in allowlist)`, so a scalar TOML string silently becomes per-character names. The gap is proof plus one hole, not a redesign.

## Requirements

**R1:** the four synthetic keys `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `AWS_ACCESS_KEY_ID`, `GITHUB_PAT` never forward even when allowlisted, despite the deny-overrides promise.

**R2:** malformed `environment.allowlist` values, including scalars, fall back safely — never crash, never char-iterate.

**R3:** URL-path redaction and descendant-process cleanup are rechecked only if directly implicated by the runner change.

**R4:** no real credentials appear in tests, code, or issue text at any point (synthetic values only; any appearance stops output and routes to Jeff's ruling).

## Key Technical Decisions

**KTD1:** scalar normalization is guarded at the single construction site, not in the shared config loader.

`_build_status_runner` is the sole consumer of `environment.allowlist`, and `cfg.get` already promises tuples for list-declared settings (talaria/config.py:22-30) — a scalar violates that contract at exactly one doorstep. Rejected alternative: normalizing in `load_config`, whose blast radius covers every list-declared setting for all consumers. Minimal scope wins per working rules; the guard coerces-or-drops non-list input to the documented default with the fallback visible, never silent char-iteration.

**KTD2:** U2 proves the existing deny-by-name boundary; it does not restructure it.

The boundary is already total by name (`_maybe_forward`, talaria/status/contract.py:184-187; `_sanitize` on the one path every variable takes, talaria/status/contract.py:142-154, including the documented attach-token overwrite lesson). The missing piece is test proof through both `build_child_env` and the `StatusRunner` tick path (talaria/status/runner.py:110-170). Restructuring a holding boundary to prove it would add risk without adding safety.

**KTD3:** rechecks fire only on implication evidence from this unit's own diff.

`redact_url` (talaria/domain/redaction.py:154) and descendant cleanup (`_release_pipes`, `_kill_process_group`, talaria/status/runner.py:405-423) are named with their exact implication test: a recheck runs only if U1/U2's change touches the value path the helper guards. Otherwise the unit records "not implicated, not rechecked" — a broad audit is the explicitly rejected alternative.

## Implementation Units

### U1. Close the scalar-allowlist hole at the construction site.

Lane: worker-6. Guard `_build_status_runner` (talaria/cli.py:302-308) so non-list `environment.allowlist` values fall back to the default instead of iterating; keep list/tuple behavior identical, including the empty default.

**Test scenarios:** scalar string, integer, and null allowlists fall back safely with identical runner behavior to unset (tests/test_config.py, tests/test_cli.py); list allowlists forward unchanged.

**Failure modes:** empty string, huge string, nested lists, and mapping values must all resolve to the safe default without raising and without forwarding fragments.

### U2. Prove deny-overrides with the four synthetic keys.

Lane: worker-6. Drive all four synthetic keys through `build_child_env` while allowlisted, plus through the `StatusRunner` construction path, asserting none forward; assert `TALARIA_*`-prefixed credential-shaped names are denied too (the prefix is not a pass, per the contract docstring).

**Test scenarios:** four-key denial via contract and runner paths (tests/status/test_env.py, tests/status/test_runner.py); prefix-is-not-a-pass case; `TALARIA_GATEWAY_URL` still sanitized on its path.

**Failure modes:** a key denied in one rule path but forwarded in another fails the unit — the assertion covers base, locale, `TALARIA_*`, and allowlist paths together, since a rule that holds per-loop but not across loops is not a boundary.

### U3. Conditional rechecks (only if implicated).

Lane: worker-6, gated on the KTD3 implication test against the U1/U2 diff. If implicated, recheck URL-path redaction and descendant cleanup on the touched paths only; if not, record "not implicated, not rechecked" with the diff evidence cited.

**Test expectation:** none unless implicated -- then targeted scenarios in the owning suite (tests/status/test_runner.py); no new suites for a negative result.

**Failure modes:** an inconclusive implication test resolves to rechecking, never to assuming safety.

## Scope Boundaries

In scope: R1–R4, the construction-site guard, deny proof, conditional rechecks, and the tests above.

Deferred to follow-up work: any recheck that fires on implication evidence ( scoped then, not now).

Non-goals: broad security audit, URL/descendant rework without implication, real-secret handling of any kind, any deferred-list widening.

## Verification

```shell
uv run pytest tests/status tests/domain
```

## Route

Proposed `backend: team-execution` (worker-6 + reviewer + tester coordinated by the controller) and wave-R1 single-PR destination — both confirm-at-tier-approval by Jeff, not decided here. Recommended next after approval: `/doc-review`, then `/work`. Saga tick is written by the controller at dispatch (no saga runner in this repo). KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval, not before.
