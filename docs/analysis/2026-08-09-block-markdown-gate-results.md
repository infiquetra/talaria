# Block-markdown gate: the two-part ownership proof, and what is not yet done

Status: `partial`
Authority: `evidence`
Date: 2026-08-09

## Verdict

**The two-part ownership proof exists, is wired into the gate, and has been proven capable of
failing.** `interface_shows_everything`'s original claim — one projected line, one on-screen
widget — is gone, replaced in `talaria/replay/gate.py` by `ownership_report`, which composes:

1. **Mounted-window ownership** (`document_ownership`, `block_documents_are_owned`): every
   currently mounted `EntryMarkdown` document — a committed entry's or either live tail's — is
   proven against its own `Markdown.source` and its own top-level blocks' `source_range` spans,
   with the blank-line accounting rule stated and pinned exactly as probed: parsing
   `"a\n\n# h\n"` yields `MarkdownParagraph (0, 1)` and `MarkdownH1 (2, 3)`; line offset 1, the
   blank line between them, sits inside neither span and is accounted to the document as a whole,
   not to either block. A construct-specific semantic oracle runs after span coverage: exact for
   fences (`MarkdownFence.code`, byte for byte) and tables (mounted `MarkdownTableCellContents`
   count against the source's own `th_open`/`td_open` count), non-emptiness-traceable-to-source
   for heading, paragraph, block quote, and both list types (row count against `list_item_open`
   count for lists).
2. **Condensed-range accounting**: the retained pane-wide identity (`mounted + condensed ==
   total`, `on_screen == projection window`) — still the right proof for line-rendered surfaces
   and for the aggregate arithmetic — plus RA3's fallback-banner charge, proven independently by
   `fallback_banner_accounting`: every maximal run of `.transcript--nowrap` line widgets in the
   mounted DOM is followed immediately by exactly one `.transcript--fallback-banner` widget, and
   no banner is mounted without a run in front of it.
3. **Zero-block sources** never reach the block proof at all: KTD2's `is_zero_block` line-renders
   them (unchanged, pre-existing behavior), so `pane.query(EntryMarkdown)` never returns one, and
   they are proven under the retained window comparison instead — confirmed directly
   (`test_expected_top_level_blocks_is_empty_for_a_zero_block_source`,
   `test_a_zero_block_entry_line_renders_and_mounts_no_entry_markdown`).

**Mutation tests prove the proof can fail**, for every mutation class KTD8's branch-hold
instruction names: dropped text (`_prove_span_coverage` with a shrunk span), a wrong block class
(a real mounted `MarkdownParagraph` reclassed to `MarkdownH1` in place), and a hidden construct —
tested per R1 construct: a fence's `.code` corrupted, a table cell removed from the live tree, a
list item row removed, and a block quote's nested content stripped. Each one turns a passing
`document_ownership` result into a failing one with a reason string naming the defect class.

`content_is_complete` (`talaria/replay/gate.py:700` in this revision) is untouched — diffed
byte-for-byte identical against the pre-U6 revision, function body and docstring both — and its
three v0.1 pin tests (`test_content_completeness_detects_a_dropped_entry`,
`test_content_completeness_matches_whole_lines_not_substrings`,
`test_content_completeness_does_not_let_an_empty_entry_match_anything`) pass verbatim. R11a holds.

**Progressiveness, R18's two-tail overlap included.** The mounted-window and banner halves of the
proof are not deferred to the settled checkpoint the way the line-window half still is — a block
document's own internal consistency is a true invariant at every instant, not a race against a
moving projection — so `measure_replay`'s sampler now calls `block_documents_are_owned` and
`fallback_banner_accounting` at every `RSS_SAMPLE_EVERY` checkpoint, not only once at the end.
`test_mounted_window_ownership_holds_mid_stream_for_two_growing_tails_at_once` drives both the
reasoning and assistant tails through three overlapping, still-growing intermediate snapshots and
proves ownership at each one.

**A genuine second measurement was added alongside the proof, not merely the proof itself.**
KTD1(a) states two ceilings — mounted widgets and descendants — and the gate's existing
`mounted_widgets` check only ever read `TranscriptPane.peak_mounted` (`len(self.children)`, a
top-level count that cannot see a table's hundreds of per-cell descendants). `TranscriptPane`
now also tracks `peak_descendants` (`descendant_count`'s own high-water mark), surfaced through
`measurements()` and gated as `descendant_widgets` / `descendant_widgets_sustained` against a new
`DESCENDANT_WIDGET_CEILING = 600`.

## Environment this was verified against

- Textual `8.2.8`, Python `3.12.11`, `Darwin 25.5.0`/`arm64` (`talaria.replay.gate.build_matrix()`).
- Terminal geometry: `GATE_SIZE = (100, 40)` (gate.py's existing pin, unchanged).
- Theme: `textual-dark` (the app's existing default; not overridden by this unit's work).

## Reproduction

```bash
uv run pytest tests/replay/test_gate.py -q          # 30 passed (17 new in this unit)
uv run pytest tests/ui/test_transcript_bounds.py tests/ui/test_transcript_blocks.py \
  tests/ui/test_blocks.py -q                         # 46 passed, no regression
uv run pytest tests/ui/ tests/replay/ tests/domain -q  # 942 passed
uv run ruff check talaria/replay/gate.py talaria/ui/transcript.py talaria/ui/app.py \
  tests/replay/test_gate.py                          # clean
uv run mypy talaria/replay/gate.py tests/replay/test_gate.py  # clean
uv run bandit -r talaria -q                           # 1 low finding, pre-existing, unchanged
  from the pre-U6 baseline (a pre-existing `assert` in transcript.py's `_MountedUnit.widgets`)
```

## What this document does not claim

This is a `partial` verdict, stated as one rather than dressed up as `pass`. The following pieces
of U6's full scope were **not completed** in this pass and are not exercised by anything above:

- **The sideband timeline.** Confirmed-cancel and typed-disconnect are not wire frames — interrupt
  replies decode to `NonEventFrame` and transport callbacks are never recorded — so AE2 at gate
  level needs a scripted action track applied alongside the frame log, with determinism proven to
  include it. Not built. `ReplaySource` and `talaria/replay/controls.py` are unchanged.
- **The KTD1(d) adversarial workload corpus**: the unbroken 100,000-character mega-line through
  resize, the 37,000-character double-width CJK line, the 601-column table, the exact-boundary
  599-column-table-plus-paragraph regression, the 302-fallen-back-entry aggregate-ceiling
  regression, and the banner-preserving odd-cut pair (round-forward and partial-retention arms at
  501 accounted rows). None of these scenarios exist yet as gate or feature-corpus fixtures. The
  descendant-ceiling *measurement* this workload would stress (`peak_descendant_widgets`,
  `DESCENDANT_WIDGET_CEILING`) is now wired and unit-tested against synthetic tables/lists, but has
  not been run against a corpus large enough to approach the ceiling.
- **Replay determinism over normalized block structure.** `run_gate`'s existing
  `replay_determinism` check still compares full domain-state equality (`AE11`, unchanged); the
  requested comparison — ordered block classes, source ranges, and semantic content, with runtime
  identifiers excluded, sideband included — was not built.
- **The feature corpus growth and the U2 stress-corpus re-baseline** — every R1 construct, early
  termination by cancel/error/typed-disconnect including the mid-table case, parser attacks, kind
  groups, and 80-column resize, folded into `talaria/replay/stress.py`'s generated corpus or a new
  fixture. Not built; `build_stress_corpus` is unchanged by this unit.
- **A full-size KTD14 gate run** (the 50,000-delta stress pass plus the sustained-cadence pass)
  against the changes here, with `verdict: pass` recorded and corpus identities cited by SHA-256.
  The reduced-scale runs in `tests/replay/test_gate.py` (600–400 deltas) continue to pass
  end-to-end with the new checks included, which is evidence the harness itself did not break, but
  it is not the KTD14-scale evidence this document's title implies and does not stand in for it.

The honest summary: the ownership proof this unit was centrally about is real, wired into the live
replay path (not only testable in isolation), and has been shown to fail on the mutation classes
named in the branch-hold instruction. The corpus growth, the sideband timeline, the normalized
determinism comparison, and the full-scale green run are outstanding work, not silently dropped
scope — each is named above so the next pass starts from an accurate map rather than rediscovering
the gap.
