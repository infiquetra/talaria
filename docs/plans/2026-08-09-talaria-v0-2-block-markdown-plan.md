---
title: Talaria v0.2 — block-level markdown and transcript differentiation plan
type: feat
status: active
date: 2026-08-09
origin: docs/brainstorms/2026-08-08-block-markdown-and-transcript-differentiation-requirements.md
---

# Talaria v0.2 — block-level markdown and transcript differentiation plan

## Summary

Render assistant and reasoning transcript entries as full block-level markdown — headings, fenced
code, lists, tables, block quotes — with Textual's built-in `Markdown` widget family, streaming
progressively, while every transcript kind group gains a distinct visual channel. The plan restates
the pane's bounded-rendering claim in an architecture decision record **before** any widget work,
splits the domain changes from the presentation changes along ADR-0002's boundary, and closes by
re-running the replay gate over a new feature corpus under the restated claim.

## Problem Frame

The transcript is constant white text; the operator's own words from live driving: "It's difficult
to read and differentiate what I am looking at." The inline-markdown decision
(docs/engineering-journal/DECISIONS.md, "Inline markdown is rendered on agent prose") recorded
"block-level rendering is taken up" as its revisit condition — this plan is that revisit. Today's
rendering is also wrong, not merely flat: the per-line styler has no fence awareness
(talaria/ui/markdown.py:82 matches code spans line-locally), so backticks inside a fence body are
styled as inline code and fence delimiter lines render literal.

The upstream requirements doc (the `origin:` above) is authoritative for the WHAT. Its R-IDs
(R1–R18) are carried forward here unchanged and are not restated in full — each unit below names
the R-IDs it discharges. The four questions the brainstorm deferred to planning are resolved as
KTD2, KTD5, KTD1's instrumentation, and KTD6 respectively.

## Grounding (what the code says today)

- `TranscriptPane` (talaria/ui/transcript.py:90) is line-indexed with four dependent mechanisms:
  the stable-prefix diff (`_common_prefix`, :270), `DEFAULT_MOUNT_CAP = 500` (:52) with
  `mounted_count` read from `len(self.children)` (:149), `_top`/`condensed_count` as an absolute
  line position (:126, :161), and `_restore_anchor` subtracting evicted widget heights (:302).
- `interface_shows_everything` (talaria/replay/gate.py:226) compares `pane.rendered_lines` against
  the projection window position-by-position and requires mounted + condensed == total; the
  mounted-widget ceiling is 600 (gate.py:69). `content_is_complete` (gate.py:290) is the
  domain-side line-indexed proof and stays untouched (R11a).
- `transcript_view` (talaria/domain/projection.py:260) flattens entries to lines, welds
  presentation prefixes (`· ` for reasoning, projection.py:301), and appends in-flight streaming
  text as a provisional `assistant` tail; `committed_lines` marks the boundary.
- `_on_error` (talaria/domain/state.py:1181) clears `streaming_text`/`reasoning_text` without
  committing them — the R6 defect, a domain transition change.
- The installed Textual is 8.2.8 (.venv, version-pinned in uv.lock). `Markdown` is exported;
  `MarkdownStream` (_markdown.py:41) is **not** exported. `Markdown.append` (:1445) is public and
  reparses from the last unfinished top-level block; `Markdown.update` (:1376) is the
  full-document replace path; blocks carry `source_range` line spans (:210, :1483) and the widget
  is a vertical container of `MarkdownBlock` children (:962). `open_links` defaults **True**
  (:1035) and `MarkdownIt("gfm-like")` defaults `html=True`, `linkify=True` — all three defaults
  are unsafe under R9/R10 and are configured off (KTD4); with `html=False` a `<script>` tag
  parses to a visible inline text token (verified live against the installed parser).

## Key Technical Decisions

**KTD1 — The bounded-rendering claim is restated as three measurable ceilings, recorded in
ADR-0006 before implementation:** (a) mounted top-level renderables — line widgets plus each entry
document's `MarkdownBlock` children plus banners — stay ≤ 600 at every instant, read from
Textual's own tree, never self-reported; (b) per-boundary reconcile work is proportional to the
provisional tail plus newly committed entries, never O(transcript); (c) under the R14 adversarial
workloads (long unclosed fence, growing table) the p99 per-boundary append/apply latency stays
under one 50 ms coalescing boundary, with high-water instrumentation (peak mounted renderables,
peak append milliseconds) exposed the way `peak_mounted` is today (transcript.py:130). Rationale:
R13 requires the target to precede the implementation; a widget-count bound alone lets one
enormous fence consume unbounded work as a single block, so work and height are bounded, not only
count.

**KTD2 — The pane goes hybrid: block documents for committed assistant/reasoning entries, line
widgets for everything else, one live tail widget for the stream.** Each committed assistant or
reasoning entry mounts one `Markdown` document (entry = parser document is exactly R15's
isolation boundary); all other kinds keep one `TranscriptLine` per line; the in-flight stream
renders in a single live `Markdown` tail widget. On commit the tail's final source becomes the
committed entry's document without a full-pane rebuild. Condensation folds from the top as today
and the banner keeps counting **lines** (the projection stays line-indexed, so
`condensed_count + lines-accounted-for == total` remains statable). Rationale: this confines
variable-height widgets to entry-scoped regions, keeps the stable-prefix diff meaningful for
line-rendered kinds, and gives the gate a per-entry region to prove ownership over (R11b).

**KTD3 — Streaming drives public `Markdown.append` at the 50 ms boundary; replacement drives
`Markdown.update`; the unexported `MarkdownStream` is not depended on.** Talaria already coalesces
paints on a 50 ms boundary, which is precisely the backpressure `MarkdownStream` exists to
provide, so pinning a private class buys nothing. `message.interim` and any replace-wins path
(R16) call `update()` — append-only cannot express replacement. A pin test asserts the observed
append/update semantics against Textual 8.2.8 and fails loudly on upgrade drift. Rationale: no
non-public API without a pinned version and an upgrade-tripwire, per the brainstorm's dependency
clause — satisfied here by not taking the non-public API at all.

**KTD4 — The parser configuration is part of the forgery boundary, and each forbidden channel
gets a proving test.** The block factory builds `Markdown(parser_factory=…, open_links=False)`
where the factory returns `MarkdownIt("gfm-like")` reconfigured with `html=False` and
`linkify=False`. Defang runs before parse at parity with today's boundary (talaria/ui/literal.py),
so C0 controls, bidi overrides, and zero-width characters never reach the parser. HTML therefore
literalizes to visible text (never disappears — R10), bare URLs stay text, links render styled but
inert, Rich console markup is never parsed over gateway bytes (the widget renders from tokens, not
markup). ADR-0005's `literal_text`-only rule is amended by ADR-0006, not silently broken.
Rationale: the widget's three unsafe defaults are all constructor-injectable; configuration plus a
test per channel is the allowlist form R9 demands.

**KTD5 — Kind differentiation is per-widget CSS classes by kind group plus theme variables, with
the channel carried by background tint and a gutter marker, not text colour.** The twelve
`TranscriptKind` members map to the five groups R7 fixes; both `TranscriptLine` and entry
documents carry a `transcript--group-*` class; assertions read computed styles (AE5), and a
fence's syntax colours cannot erase a background/gutter channel (R8). Styling changes no widget's
height, measured against the same screen with styling disabled. Rationale: classes compose with
the widget family's own styles where text-colour overrides would fight the highlighter.

**KTD6 — The projection grows an entry-scoped surface; the flattened line buffer keeps its
decorated lines exactly as today.** `TranscriptView` adds per-entry records — kind, raw body,
committed flag, line span — for the widget layer, and the reasoning `·` weld becomes decoration
applied only to line-rendered surfaces (R18: a first-line heading in a reasoning stream must be
valid markdown). `terminal_read` (KTD10 of v0.1) and `content_is_complete` (R11a) read the
unchanged decorated line buffer, so the agent-facing read and the domain proof move zero bytes.
Rationale: two consumers, two surfaces — restating the line buffer would break the v0.1 pin
`tests/domain/test_projection.py::test_every_transcript_entry_survives_into_the_line_buffer`,
which must keep passing verbatim.

**KTD7 — Terminal paths commit partial buffers in the domain, distinguishing terminal from
transient.** `_on_error` (state.py:1181), the confirmed-cancel transition, and terminal disconnect
commit `streaming_text` and `reasoning_text` as entries before clearing (R6); a transient
reconnect that resumes the same response commits nothing and must not duplicate. This is a
domain-only change under ADR-0002 — no framework import.

**KTD8 — The styled-line-run fallback is gate-triggered and amends R4 explicitly.** The fallback
(projection-tagged line runs styled by the existing pane) is invoked only on a red restated gate
with the measured evidence attached, and taking it is a recorded requirements amendment in
DECISIONS.md — never a silent swap. Until that evidence exists, no fallback code is written.

## Dependency order

The spec graph in the companion execution spec is authoritative for execution order. The shape:
U1 (the ADR), U2 (domain), and U3 (the block factory) are mutually file-disjoint and start
together; U4 (the hybrid pane) needs all three; U5 (kind styling) edits the same files as U4 and
follows it; U6 (the restated gate and corpus) closes. Each build unit is cross-reviewed by the
Codex engine before its dependents run, mirroring the answerability spec's review-gating.

## Implementation Units

### U1. ADR-0006 — the block-aware bounded-rendering claim

**Goal:** Record the restated claim, its instrumentation, and its ceilings before any
implementation lands on it (R13; KTD1, KTD8).

**Files:** platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md
(new), docs/engineering-journal/DECISIONS.md (KTD mirror).

**Scope:** The three ceilings of KTD1 with their exact measurement points (the tree-read mounted
count, the reconcile proportionality argument, the 50 ms p99 latency ceiling and its workloads);
the amendment to ADR-0005's `literal_text`-only rule (untrusted text may additionally reach the
screen through the KTD4-configured parser and nothing else); the fallback trigger (KTD8) and what
evidence invokes it.

**Test expectation:** none — architecture record; U6 makes every clause executable.

### U2. Domain: terminal-path commits and the entry-scoped projection

**Goal:** Commit partial buffers on turn-terminal paths and publish raw entry bodies beside the
decorated line buffer (R6, R18; KTD6, KTD7).

**Files:** talaria/domain/state.py, talaria/domain/projection.py, tests/domain/test_state.py,
tests/domain/test_projection.py.

**Test scenarios (tests/domain/test_state.py, tests/domain/test_projection.py):** an error
arriving mid-fence commits the partial streaming and reasoning buffers as entries, then clears
(AE2's error leg); a confirmed cancellation commits likewise; a transient reconnect resuming the
same response produces the content exactly once; `message.interim` replacement respects the
committed/interim boundary; the entry-scoped view carries a reasoning body with **no** `·` weld
while the flattened line buffer keeps it; the v0.1 pin
`test_every_transcript_entry_survives_into_the_line_buffer` passes unmodified.

### U3. The block factory and its forgery proof

**Goal:** One constructor for safely configured entry documents, with a test per forbidden
channel (R9, R10, R15; KTD3, KTD4).

**Files:** talaria/ui/blocks.py (new), tests/ui/test_blocks.py (new).

**Test scenarios (tests/ui/test_blocks.py):** the AE3 quartet — `<script>alert(1)</script>`
visible as text, `[bold red]x[/]` unparsed, a raw ANSI escape defanged, `[click](https://example.com)`
styled but inert with nothing opened or fetched; a bare URL stays text (linkify off); defang runs
before parse (a NUL/bidi payload arrives as control pictures in block content); two entries with
an unclosed fence in the first never absorb the second (R15); the Textual 8.2.8 pin test — append
reparses from the last unfinished block, update replaces — fails on semantic drift.

### U4. The hybrid transcript pane

**Goal:** Mount committed assistant/reasoning entries as block documents, keep line widgets for
every other kind, stream the tail progressively, and restate cap, condensation, and anchor over
mixed units (R1, R2, R5, R16, R17; KTD1, KTD2, KTD3).

**Files:** talaria/ui/transcript.py, talaria/ui/app.py, tests/ui/test_transcript_blocks.py (new),
tests/ui/test_transcript_bounds.py.

**Test scenarios (tests/ui/test_transcript_blocks.py, tests/ui/test_transcript_bounds.py):** AE1 —
a fence streamed opener/body/closer renders structure at each surviving boundary, no inline
code-span styling inside the fence, one bounded region when closed; an interim replacement shows
the authoritative text exactly once (AE7); commit hands the tail source to the entry document
without a pane rebuild; the mounted-renderable count under the cap at every instant including
mid-mount (the KTD1 tree-read); condensation folds whole units and the banner's line arithmetic
still sums; a reader scrolled above the stream holds position through reinterpretation, resize,
and condensation (R17) and follow-bottom is never stolen; tables render and their cells are
keyboard-reachable at 80 columns (AE4/R3).

### U5. Kind differentiation

**Goal:** Give the five kind groups their named visual channels, composed with block rendering
(R7, R8; KTD5).

**Files:** talaria/ui/transcript.py, talaria/ui/app.py (CSS), tests/ui/test_kind_styles.py (new).

**Test scenarios (tests/ui/test_kind_styles.py):** AE5 — one line of each group on screen,
adjacent groups distinguishable by computed style with content ignored; a reasoning fence carries
both the kind channel and fence rendering (R8); per-widget height equal with styling on and off;
the twelve-member mapping is total (a new `TranscriptKind` member fails the mapping test rather
than silently rendering ungrouped).

### U6. The restated gate, the feature corpus, and the green re-run

**Goal:** Replace `interface_shows_everything`'s one-line-one-widget claim, grow the corpus this
feature needs, and re-run the gate green (R5, R11b, R12, R13, R14, R17; AE6).

**Files:** talaria/replay/gate.py, talaria/replay/stress.py, tests/replay/test_gate.py,
tests/ui/test_transcript_bounds.py, docs/measurements/ (results doc beside the existing gate
records).

**Test scenarios (tests/replay/test_gate.py plus the gate's own corpus verdict):** the
region-ownership proof — every projected source region of a committed block entry is owned by a
mounted, visible block whose `source_range` covers it, with construct-specific visual assertions
(a table renders a grid, a fence renders a bounded region); line-rendered kinds keep the existing
window comparison; progressiveness asserted at timed intermediate checkpoints, not only settled
(R5/R14); the adversarial workloads — long unclosed fence, growing table — hold KTD1's latency
and mount ceilings with high-water figures recorded; replay determinism compares normalized block
structure (ordered classes, source ranges, semantic content; runtime identifiers excluded) under
pinned width, theme, and framework version (R12); early termination by cancellation, error, and
disconnect renders all received content (AE2 at gate level); resize including 80 columns; verdict
green over both the existing corpora and the new feature corpus.

## Scope Boundaries

Carried from the brainstorm unchanged: link activation, URL fetching, and images stay disabled
behaviors; diff rendering keeps RR-38's plain-text stance; theme selection and terminal colour
detection stay deferred; turning fence highlighting off is out of scope; no Hermes markdown
machinery is ported (ADR-0003). Additionally out of scope for this plan: the approved v0.2
answerability plan and its execution spec are untouched; transcript memory eviction (KTD14's
deferred item) is not taken up; the styled-line-run fallback is not built ahead of the evidence
that would invoke it (KTD8).

Deferred to follow-up work: exposing the block structure to `terminal_read` (the agent-facing
buffer stays line-indexed); markdown in `tool` or `user` entries (the inline decision's reasoning
still binds); underscore emphasis (the inline decision's revisit clause governs).

## Risks

- **The widget family may not coexist with the pane's mechanics at acceptable cost** — this is
  the brainstorm's named assumption, it is exactly what U6 measures, and KTD8's fallback exists
  because it may measure false. The fallback decision point is after U6's first red run, with
  numbers attached, never before.
- **Textual upgrade drift** — append/update semantics are internal implementation detail of a
  public method; the U3 pin test converts silent drift into a loud failure.
- **The gate's new claims could be weaker than the old ones** — mitigated by keeping
  `content_is_complete` and the v0.1 projection pin untouched (R11a) and by the review gate on U6
  before the verdict is trusted.
