---
title: Talaria v0.6.0 tier-T2 plan — configurable appearance and Homebrew (#123)
type: feat
status: active
date: 2026-09-03
origin: infiquetra/talaria#123 (requirements ledger on infiquetra/talaria#118, findings F8–F9, recorded decision C)
backend: team-execution
---

# Talaria v0.6.0 tier-T2 plan — configurable appearance and Homebrew (#123)

Summary: add sparse theme inheritance (shared defaults, groups, overrides), independent category/marker/background controls, space-reclaiming offset removal, and a fifth Homebrew theme — with existing defaults, readability, and startup selection all preserved.

## Run placement

Tier T2 (with #120 + #124), review wave R2 (one frozen target, one Saga Code Review for all three). Base `8d9747dac` (= origin/main = integration head, verified 2026-09-03; R1 branches touch none of this unit's surfaces — re-anchor at dispatch regardless). Downstream content gate into #124 (imports map onto this inheritance contract). No implementation starts before Jeff approves this tier. Decision C (host-palette inheritance where terminal colors apply, Talaria-specific overrides, readability and sparse overrides preserved) is a settled input, never re-opened.

## Problem Frame

Four built-ins (`refined-default`, `dark-green-terminal`, `neutral-dark`, `accessible-high-contrast`; talaria/themes/builtins.py:12-210, docs/themes.md:1-18) render through one fixed mapping with no inheritance and no per-category control, so operators cannot tune appearance without forking a theme; and the transcript pane (talaria/ui/transcript.py, 2007 lines: `TranscriptLine` :486, `TranscriptPane` :692) owns category color, markers, and the left offset with no shared-defaults layer for #124's imports to target.

## Requirements

**R1:** sparse inheritance across shared defaults, groups, and individual overrides; existing themes preserve defaults.

**R2:** independent category body text colors; independent stripe/marker and background controls.

**R3:** optional left-edge offset removal that reclaims the space.

**R4:** Homebrew fifth built-in — restrained green-black palette with documented mapping plus provenance — available by default, NOT startup-selected.

**R5:** terminal theme mode inherits the host palette where terminal colors meaningfully apply, with Talaria-specific overrides, readability, and sparse overrides preserved (decision C).

## Key Technical Decisions

**KTD1:** inheritance resolves in the registry path so #124 shares one semantics.

`ThemeRegistry.resolve` (talaria/ui/theme.py:150) is the single funnel every theme — built-in, user, and imported — passes through. Resolving shared-defaults → groups → overrides there (rather than in each consumer) means #124's imported `ThemeSpec`s (talaria/themes/__init__.py:101) inherit the identical behavior with no second implementation.

**KTD2:** the readability floor reuses the existing contrast machinery.

`relative_luminance`/`contrast_ratio` (talaria/ui/theme.py:25-37) already exist; category, marker, and background overrides resolve against them so no override combination silently destroys legibility. Rejected alternative: a new readability subsystem — unneeded machinery for a floor the codebase already computes.

**KTD3:** offset removal reclaims layout space, not just paint.

Hiding the offset while leaving the gutter width in the layout would satisfy a screenshot and fail the requirement ("reclaims the space"). The change lands in the pane layout (`TranscriptPane`, talaria/ui/transcript.py:692), verified by width assertions, not pixel inspection.

**KTD4:** Homebrew is a fifth `ThemeSpec` beside `DARK_GREEN_TERMINAL`, with the default untouched.

`DARK_GREEN_TERMINAL` (talaria/themes/builtins.py:78) is an existing aesthetic, not the Homebrew brief; Homebrew gets its own spec with documented palette mapping plus provenance, while the registry default (talaria/ui/theme.py:147, fallback `refined-default` per docs/themes.md) does not move.

## Implementation Units

### U1. Sparse inheritance across defaults, groups, overrides.

Lane: worker-6. Implement resolution order in the registry path per KTD1; existing four themes resolve byte-identical to today.

**Test scenarios:** override-beats-group-beats-default chains; sparse specs (unset keys fall through, never reset); all four existing themes preserve defaults (tests/ui/test_theme.py, new tests/themes/ suite — created by this unit's first test commit).

**Failure modes:** empty groups, unknown keys, and null values fall through safely; a malformed override never breaks resolution of the rest.

### U2. Category text colors plus marker/background controls.

Lane: worker-6. Independent category body text colors and independent stripe/marker/background controls on the transcript surface (`kind_group` talaria/ui/transcript.py:305, `TranscriptLine` :486), gated by the KTD2 contrast floor.

**Test scenarios:** per-category colors render independently; marker/background vary independently; low-contrast combinations hold the floor (tests/ui/test_theme.py, test_kind_styles.py, test_transcript_blocks.py).

**Failure modes:** unreadable combinations resolve to the floor rather than rendering; unknown categories fall back to the group default.

### U3. Space-reclaiming offset removal.

Lane: worker-6. Optional left-edge offset removal in the pane layout per KTD3, with width assertions proving the reclaim.

**Test scenarios:** offset on/off widths differ by exactly the offset; content reflows into reclaimed space (tests/ui/test_transcript_bounds.py).

**Failure modes:** narrow terminals and wrapped rows must not overflow or clip when the offset is removed.

### U4. Homebrew theme, docs, and host-palette inheritance.

Lane: worker-6. Fifth `ThemeSpec` per KTD4 with documented mapping plus provenance; `docs/themes.md` updated (built-in table, inheritance rules, Homebrew section); host-palette inheritance applied where terminal colors meaningfully apply with Talaria-specific overrides preserved.

**Test scenarios:** Homebrew present and selectable, never startup-selected; default resolution unchanged; inheritance surfaces documented (tests/ui/test_theme.py, tests/themes/).

**Failure modes:** unresolvable host palette degrades to the built-in mapping with a notice, never a crash or a blank theme.

## Scope Boundaries

In scope: F8–F9, decision C as specified, the surfaces and tests above, `tests/themes/` creation.

Deferred to follow-up work: nothing currently — any follow-up is re-derived live, not pre-listed.

Non-goals: startup-selection change, existing-default breakage, unrelated theme redesign, deferred-list widening.

## Verification

```shell
uv run pytest tests/ui tests/themes
```

## Route

Proposed `backend: team-execution` and wave-R2 single-PR destination — confirm-at-tier-approval by Jeff. Recommended next after approval: `/doc-review`, then `/work`. Saga tick by the controller at dispatch. KTDs transfer to `docs/engineering-journal/DECISIONS.md` after tier approval.
