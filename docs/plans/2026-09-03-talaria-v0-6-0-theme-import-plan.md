---
title: Talaria v0.6.0 tier-T2 plan — theme import and live reload (#124)
type: feat
status: active
date: 2026-09-03
origin: infiquetra/talaria#124 (requirements ledger on infiquetra/talaria#118, finding F7, recorded decision D)
backend: team-execution
---

# Talaria v0.6.0 tier-T2 plan — theme import and live reload (#124)

Summary: keep local VSCode JSON import and add bounded marketplace fetch, in-app `/theme` selection, and explicit live Reload — one parse path, data-only packages, no watcher, failures preserve the current theme.

## Run placement

Tier T2 (with #120 + #123), review wave R2 (one frozen target, one Saga Code Review for all three). Base `8d9747dac` (= origin/main = integration head, verified 2026-09-03). Re-anchor warning: R1's `work/126-harden` touches `talaria/cli.py` and `talaria/status/contract.py` — this unit also touches `talaria/cli.py`, so it re-integrates the R1 merge at dispatch and re-anchors CLI line refs before editing. Content gate from #123 (imports map onto the inheritance contract — starts only after #123 acceptance evidence is recorded). Same-wave `talaria/ui/app.py` sharing with #120 is sequenced by the architect. No implementation starts before Jeff approves this tier. Decision D (accept the user-selected source, no additional trust policy) is a settled input, never re-opened.

## Problem Frame

Import exists but stops at the process boundary: the CLI offers `theme import` for one strict VSCode JSON file (talaria/cli.py:146-164, "restart-scoped"), the parser produces reports (`ImportReport`, talaria/ui/theme_import.py:192; entry `prepare_vscode_theme_import` :707, `import_vscode_theme` :756), and user specs persist via storage (talaria/themes/storage.py:21-103) — but there is no marketplace fetch, no in-app `/theme` path (the `command.action == "theme"` handler at talaria/ui/app.py:5173 serves the existing surface), and no Reload without restart. The format contract already promises "never watched after import" (docs/formats/vscode-theme-import.md:1-8); Reload must honor that promise while removing the restart.

## Requirements

**R1:** local VSCode JSON import preserved; marketplace search/select/fetch added with no manual download step.

**R2:** `/theme` access with immediate preview/select; explicit live Reload without restart and without a background watcher.

**R3:** packages treated as data and never executed; invalid or failed data preserves the current theme.

**R4:** user-selected source accepted with no additional trust policy (decision D).

## Key Technical Decisions

**KTD1:** fetch is transport only — one parse path for local and marketplace input.

`prepare_vscode_theme_import` (talaria/ui/theme_import.py:707) already turns strict JSON into a spec plus report. Marketplace fetch delivers bytes into that same entry point rather than growing a second parser, so the strictness rules, alpha compositing, and report lines in docs/formats/vscode-theme-import.md hold identically for both sources. Data-only is then structural (bytes in, spec out, nothing executed) rather than a policy promise.

**KTD2:** Reload re-reads on explicit user action; the no-watcher contract stands.

The format doc's "never watched after import" is a public promise, so Reload re-runs the import pipeline for the known source on demand instead of observing the filesystem. Rejected alternative: a file watcher — directly contradicts the published contract.

**KTD3:** imported specs resolve through #123's inheritance semantics.

Per the content gate, an imported `ThemeSpec` is subject to the same shared-defaults → groups → overrides resolution as built-ins, so marketplace themes compose with operator overrides instead of bypassing them. If #123's resolution changes during review, this unit re-anchors to it before merging.

**KTD4:** failure preserves the current theme at every step.

Network failure, parse failure, and out-of-allowlist content each resolve to "keep rendering the current theme with a notice" — the same discipline `decode_catalog` applies to odd listings. No partial application: a failed import changes nothing.

## Implementation Units

### U1. Bounded marketplace search/select/fetch into the existing parse path.

Lane: worker-6. Fetch layer delivering source bytes to `prepare_vscode_theme_import` per KTD1; search/select UI bounded to the approved scope; accept-source per decision D with no trust-policy invention.

**Test scenarios:** fetched bytes parse identically to the same file on disk; search/select/fetch round trip without manual download; malformed payloads rejected as data (tests/ui/test_theme_import.py).

**Failure modes:** network failure, non-JSON bodies, oversized payloads, and unknown sources all preserve the current theme with a notice; fetched bytes are never executed, imported, or dynamically loaded as code.

### U2. In-app `/theme` access with immediate preview/select.

Lane: worker-6. Extend the existing `command.action == "theme"` surface (talaria/ui/app.py:5173) with select plus immediate preview; persistence stays an explicit action per the docs/themes.md contract ("preview is immediate, persistence is always explicit").

**Test scenarios:** select previews immediately without persisting; explicit persist writes via storage (talaria/themes/storage.py:21-43); invalid selections keep the current theme (tests/ui/test_theme.py, test_theme_import.py).

**Failure modes:** preview of a broken spec shows the notice and keeps rendering the old theme; persistence failures never leave a half-written spec.

### U3. Explicit live Reload without restart or watcher.

Lane: worker-6. Reload command re-running the import pipeline for the known source per KTD2; app applies the re-resolved theme live.

**Test scenarios:** edited source re-imports on Reload without restart; no background watcher exists (assert no watcher registration); Reload of a now-invalid source preserves the current theme.

**Failure modes:** Reload mid-streaming-turn applies cleanly or defers with a notice — never tears down the live session; concurrent Reload presses serialize.

### U4. Format-doc and user-doc updates.

Lane: worker-6. Extend docs/formats/vscode-theme-import.md (marketplace source rules under the same strictness) and docs/themes.md (import/select/Reload flow); public-safe, no policy text.

**Test expectation:** none -- docs; doc checks at closeout. Any code example in docs is copied from the tested path, never hand-written.

**Failure modes:** doc claims drift from behavior is caught at the R2 frozen-target review, which reads docs against the implementation.

## Scope Boundaries

In scope: F7, decision D as specified, the surfaces and tests above.

Deferred to follow-up work: nothing currently — any follow-up is re-derived live, not pre-listed.

Non-goals: file watcher, extension-code execution, trust-policy invention, theme-format redesign, deferred-list widening.

## Verification

```shell
uv run pytest tests/ui
```

## Route

Proposed `backend: team-execution` and wave-R2 single-PR destination — confirm-at-tier-approval by Jeff. Recommended next after approval: `/doc-review`, then `/work`. Saga tick by the controller at dispatch. KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval.
