# Block-markdown gate: the two-part ownership proof, the sideband timeline, and a real defect it found

Status: `partial`
Authority: `evidence`
Date: 2026-08-09

## Verdict

**All four of U6's outstanding gaps are built and exercised; one of them surfaced a genuine,
pre-existing rendering defect that this document reports rather than hides, and one full-scale
KTD14 run has not yet been executed in this pass.** The two-part ownership proof (the previous
revision of this document) is unchanged and still holds — see that section below. New in this
revision:

1. **The sideband timeline** (`talaria/replay/source.py`) — a scripted, non-wire-frame action
   track for confirmed-cancel and typed-disconnect, fired deterministically by `ReplaySource`
   itself at the exact frame index each action targets, applied by
   `talaria/replay/gate.py`'s `_apply_sideband_action`.
2. **The KTD1(d) adversarial workload harness** (`talaria/replay/workloads.py`, new module) — the
   three named growth curves and three boundary probes, each measured with `time.monotonic()`
   around `TranscriptPane.apply`, with high-water figures recorded.
3. **Replay determinism over normalized block structure** (`normalized_block_structure`,
   `talaria/replay/gate.py`) — R12's additional comparison, alongside the existing full
   domain-state equality check.
4. **The feature corpus** (`build_feature_corpus`, `talaria/replay/stress.py`) — every R1
   construct, early termination by cancel/error/typed-disconnect (mid-table included), the AE3
   parser-attack quartet, and a representative of every R7 kind group, scripted and deterministic.

Building the feature corpus's table turn honestly (streamed across three separate deltas, the way
a real gateway would send one) found a real, previously undiscovered defect in the installed
widget's append mechanics — not something this unit introduced or is in scope to fix. It is
described in full under "A real defect this work found" below, and it is why
`feature_corpus_content_loss` is an expected, named failure in every gate run below rather than a
green checkmark. **A full 50,000-delta KTD14-scale gate run (this document's historical
`docs/analysis/2026-08-03-textual-validation-gate-results.md` precedent) has not been executed in
this pass** — every number below is from the test suite's own reduced-scale runs and from direct
invocation of the new measurement functions at their plan-specified sizes, which is real evidence
but is not the same claim as a full KTD14 pass. That gap is named, not silently absorbed.

## Part 1: the two-part ownership proof (unchanged from the prior revision)

`interface_shows_everything`'s original claim — one projected line, one on-screen widget — is
gone, replaced in `talaria/replay/gate.py` by `ownership_report`, which composes:

1. **Mounted-window ownership** (`document_ownership`, `block_documents_are_owned`): every
   currently mounted `EntryMarkdown` document — a committed entry's or either live tail's — is
   proven against its own `Markdown.source` and its own top-level blocks' `source_range` spans,
   with the blank-line accounting rule stated and pinned exactly as probed: parsing
   `"a\n\n# h\n"` yields `MarkdownParagraph (0, 1)` and `MarkdownH1 (2, 3)`; line offset 1, the
   blank line between them, sits inside neither span and is accounted to the document as a whole,
   not to either block. A construct-specific semantic oracle runs after span coverage: exact for
   fences (`MarkdownFence.code`, byte for byte) and tables (mounted `MarkdownTableCellContents`
   count against the source's own `th_open`/`td_open` count), non-emptiness-traceable-to-source
   for heading, paragraph, block quote, and both list types.
2. **Condensed-range accounting**: the retained pane-wide identity (`mounted + condensed ==
   total`, `on_screen == projection window`) plus RA3's fallback-banner charge
   (`fallback_banner_accounting`).
3. **Zero-block sources** never reach the block proof at all — they line-render per KTD2 and are
   proven under the retained window comparison instead.

Mutation tests prove the proof can fail, for dropped text, a wrong block class, and a hidden
construct per R1 kind. `content_is_complete` is untouched (R11a) — see "Checks" below for the
verbatim confirmation run in this pass.

**One correctness fix landed inside this proof in this revision.** `_construct_oracle`'s fence
oracle (`talaria/replay/gate.py`, in `_construct_oracle`) used to compute a closed fence's expected
body by slicing `body_lines[1:-1]` — stripping the first line as the opening delimiter and the
*last* line as a closing one, unconditionally. An **unclosed** fence has no closing delimiter, so
that slice silently ate one real line of content every time a fence never closed — exactly the
shape a confirmed-cancel or typed-disconnect mid-fence commits, and exactly what U6's own
mid-fence sideband scenario produces. The fix re-parses the block's own span with the identical
parser and reads the resulting `fence` token's own `.content` — correct whether the fence is
closed or not, because markdown-it's own fence parsing already handles both cases; no more hand-
rolled delimiter arithmetic that has to get the closed/unclosed distinction right a second time.
Pinned by `test_an_unclosed_fence_with_real_trailing_content_is_not_a_false_positive`
(`tests/replay/test_gate.py`). Found and fixed while building gap 1's mid-fence cancellation
scenario below — without it, that scenario reported a false ownership failure on content that was
actually rendering correctly.

## Part 2: the four gaps

### Gap 1 — the sideband timeline (AE2 at gate level)

**Built.** Confirmed-cancel and typed-disconnect are not wire frames: an interrupt reply decodes to
`NonEventFrame` and the reducer ignores it, and there is no `gateway.disconnected` event type —
`note_connection_state` is a transport *callback*, never a recorded frame. `talaria/replay/source.py`
adds `SidebandAction` (`frame_index`, `kind` — `confirmed_cancel` or `typed_disconnect` — and an
optional `cause`) and `build_sideband()`, which validates and totally orders a scripted action
track. `ReplaySource.__aiter__` fires the callback for a matching `frame_index` immediately *after*
the consumer has processed that frame (the generator resumes past its `yield` only once
`TalariaApp.ingest()` has returned for that frame), which is what makes the ordering deterministic —
it is the source's own pacing deciding when an action fires, not a concurrent poll racing the
consumer. `ReplaySource.bind_sideband()` lets a caller arm the timeline after both the source and
the app that will consume it exist (the app needs the source to construct; the callback needs the
app to apply an action to).

`talaria/replay/gate.py`'s `_apply_sideband_action` is where an action actually touches domain
state. A confirmed-cancel calls `cancel_turn` directly (`talaria.domain.state.cancel_turn`) — the
same function a live interrupt confirmation would call, and the only faithful choice: replay's own
`action_interrupt` is deliberately refused in replay mode (AE11 — there is no gateway to
interrupt), and that refusal means "the operator pressed interrupt during this replay", not "this
recorded history already contains a cancellation", which is what a sideband action represents. A
typed-disconnect calls the *public* `TalariaApp.note_connection_state`, the exact method the live
wiring (`talaria/cli.py`'s `source.bind(on_connection=app.note_connection_state, ...)`) calls —
full fidelity to the real call path, not a domain-function shortcut.

**Test scenarios** (`tests/replay/test_gate.py`): `test_sideband_actions_fire_deterministically_after_their_own_frame_index`
(a trailing action scheduled beyond the corpus length still fires once, at the end);
`test_build_sideband_rejects_two_actions_on_the_same_frame_index`;
`test_sideband_action_validates_its_own_cause_pairing`;
`test_apply_sideband_action_calls_the_domain_transition_a_live_confirmation_would`;
`test_replaying_a_sideband_bearing_corpus_twice_produces_identical_state`;
`test_early_termination_by_cancel_and_disconnect_commits_partial_content` (drives the real feature
corpus and confirms every early-termination arm's content survived, mid-table cancellation
included). All pass.

### Gap 2 — the KTD1(d) adversarial workloads

**Built**, with one honest scale caveat. `talaria/replay/workloads.py` (new module) implements the
three named growth curves at their exact specified sizes — a growing unclosed fence, 100 lines per
boundary to 10,000 lines (`growing_fence_boundaries`); a one-column table, 10 rows per boundary to
1,000 rows (`growing_table_boundaries` — one column specifically, matching the plan's own exact
"1,003 descendants" figure, which only holds for a single-column table); a single unbroken line,
5,000 characters per boundary to 100,000 (`growing_line_boundaries`, exercised through an
80-column resize a third of the way through, via `run_adversarial_workloads`'s `resize_at`) — plus
the three boundary probes named in KTD1(a)'s own grounding: the 37,000-character double-width CJK
line, the 601-column table, and the exact-boundary 599-column-table-plus-paragraph regression.
`measure_apply_latency` clocks `time.monotonic()` strictly around `await pane.apply(view, entries)`,
excludes the first `WARMUP_BOUNDARIES` (10) samples, and reports the nearest-rank p99 of the rest
against `LATENCY_CEILING_MS` (50ms). High-water figures recorded per boundary run: peak descendants
(tier two — the single live tail this harness grows; tier one, the folded window, is proven
separately by the existing 302-entry/odd-cut/partial-retention real-fold regressions in
`tests/replay/test_gate.py` from the prior pass), peak `EntryMarkdown.append` reparse-window bytes
(`reparse_window_bytes`, mirroring `Markdown.append`'s own `_last_parsed_line`-anchored formula
exactly — see the defect section below for why that number matters), tallest painted row count, and
whether the two-condition fallback trigger fired.

**A second real, in-scope finding surfaced by this workload, and corrected**: the plan's own text
reads "the gate's 1,000-row table workload measures 1,003 descendants, fitting with headroom" next
to the 1,200 descendant-trigger threshold — building the workload and running it confirms 1,003 is
exactly right, **but** the table still trips the fallback trigger, because the trigger has a second,
independent condition: 1,000 rows is 1,002 source lines regardless of column width, and the
width-aware wrapped-row estimate counts at least one row per source line, clearing
`DEFAULT_MOUNT_CAP` (500) on line count alone. The plan's parenthetical is a claim about the
descendant metric specifically (proving it alone would not have caught this shape, which is
exactly why the trigger has two conditions); it is not a claim that the workload avoids the
trigger overall. `talaria/replay/workloads.py`'s `LatencyReport.fell_back` docstring and
`test_the_one_column_table_workload_holds_under_the_descendant_trigger_at_full_size`
(`tests/replay/test_gate.py`) both state this precisely now, verified at the full 1,000-row size.

**The honest scale caveat**: `test_the_601_column_table_boundary_probe_estimate_exceeds_the_trigger`,
`test_the_exact_boundary_599_column_regression_fires_the_trigger`, and
`test_the_one_column_table_workload_holds_under_the_descendant_trigger_at_full_size` all ran at
their full, plan-specified sizes directly (not through `run_adversarial_workloads`, and not
reduced) and pass — 1,204 and 1,201 descendants respectively for the two table probes, both firing
the trigger; 1,003 descendants and a tripped wrapped-row trigger for the full 1,000-row table. The
full 10,000-line fence and 100,000-character mega-line workloads, run end-to-end through
`run_adversarial_workloads`, have only been exercised at *reduced* scale
(`test_run_adversarial_workloads_covers_every_named_workload_and_probe`, monkeypatched to 300
lines / 100 rows / 15,000 characters for test speed) — a full-size `run_adversarial_workloads()`
invocation, with its own published p99 figures at 10,000/1,000/100,000, has not been run and
recorded in this pass. The mechanism, the sizes, and the ceiling comparison are all real and
tested; the specific full-size latency numbers for the fence and mega-line workloads are not yet
published.

### Gap 3 — replay determinism over normalized block structure (R12)

**Built.** `normalized_block_structure` (`talaria/replay/gate.py`) derives, from a `SessionState`
alone (no live pane needed), an ordered tuple of `(kind, block_class_name, source_start,
source_end, semantic_content)` for every committed entry KTD2's own mounting rule
(`_ktd2_selects_block`) would render as a block, using the same `_expected_top_level_blocks` parser
call the ownership proof already uses. Entry ids are deliberately absent from the tuple — that is
what "runtime identifiers excluded" means, and it is proven directly:
`test_normalized_block_structure_excludes_entry_ids_but_catches_dropped_content` builds two states
with identical content under different entry ids and asserts equal normalized structure, then
drops content and asserts inequality. `run_gate()` computes it across the same three
sideband-bearing replay treatments (64x, 64x-with-pause, unbounded) the full-state
`sideband_replay_determinism` check uses, as the additional `sideband_normalized_structure_determinism`
check — sideband included, since a sideband-committed entry is an ordinary transcript entry that
flows through `entry_scoped_view` like any other.

**Test scenarios**: `test_feature_corpus_sideband_replay_agrees_on_normalized_block_structure`. Both
this and the full-state sideband determinism check pass against the real feature corpus.

### Gap 4 — the feature corpus and the U2 re-baseline

**Built.** `build_feature_corpus()` (`talaria/replay/stress.py`) is a fully scripted, deterministic
47-frame corpus (label `talaria-feature-v1`, sha256
`de9d6f55d54e166c559dc3c7228fcfb665ef6d5cdec042b7648716de5465e12a` for this revision) covering:

- Every R1 construct: heading + paragraph + RA1's emphasis/strikethrough allowlist; both list
  types; block quote; fence region (closed); table grid (streamed progressively across three
  deltas, closed normally — see the defect section for why that shape matters).
- The AE3 parser-attack quartet plus a bare URL and an image, in the block-quote turn.
- Every R7 kind group: operator (a replayed outbound `prompt.submit`), assistant, reasoning
  (alongside assistant in the same turn, R18), activity (`tool.start`/`tool.complete`, and a
  subagent fan-out), session-record and fault (protocol-error, unknown-event).
- Early termination by confirmed-cancel (plain, and the AE2-named mid-table case — cancelled right
  after the header/separator/first row), by a real `error` wire event, and by typed-disconnect
  (`orderly_close`).

`test_the_feature_corpus_covers_every_r1_construct_and_every_kind_group` pins the shape directly
against the generated frames. One mechanical fix was needed to make the corpus usable at all: every
turn originally used a distinct per-turn session id, which `_apply_event`'s cross-talk guard
(`talaria/domain/state.py`) silently drops after the first session id it ever sees on the wire —
fixed by using one constant `FEATURE_SESSION_ID` for the whole corpus, matching
`build_stress_corpus`'s own pattern.

**The U2 re-baseline**, addressed directly rather than deferred: U2's terminal-path-commits-partials
change means any corpus replayed through the current reducer reports different projected-line and
entry counts than a pre-U2 baseline would have. This document's own reproduction commands (below)
are that re-baseline — every figure here is measured against the current code, not carried forward
from the 2026-08-03 or earlier 2026-08-09 revisions of this document. No hardcoded pre-U2 count
exists anywhere in `talaria/replay/stress.py` or the test suite that needed updating; the
re-baseline is the act of citing fresh numbers, which this document does throughout.

## A real defect this work found, precisely reproduced, out of scope to fix

**Building the feature corpus's table turn honestly — streamed across three separate deltas, the
way a real assistant response actually arrives — surfaced a genuine, pre-existing defect in the
installed Textual widget's construction-vs-append bookkeeping.** It is not something this unit
introduced (reproduced against a bare, unwrapped `textual.widgets.Markdown`, not only
`talaria/ui/blocks.py`'s `EntryMarkdown`), and fixing it means editing `talaria/ui/transcript.py`
or `talaria/ui/blocks.py`, both explicitly out of this unit's scope. It is reported here in full
rather than routed around by simplifying the corpus to a shape that would not find it.

**Mechanism.** Textual 8.2.8's `Markdown.__init__` seeds initial content through the same code path
as `update()`, which sets the private `_last_parsed_line` bookkeeping with a heuristic blind to
which construct is still open — "total source lines minus one, if the last line is non-empty"
(`_markdown.py:1435-1436`). That heuristic is correct only when the construct is exactly one line
at construction time. A live streaming tail that first becomes block-eligible with a multi-line
table or fence already in it — the *only* way a table can become eligible at all, since
markdown-it needs a header, a separator, and at least one row before it recognizes a table — is
seeded through exactly that path. The next `EntryMarkdown.append` call then reparses a window that
starts *inside* the construct with no header or fence-open marker in view, which markdown-it parses
as a bare paragraph, and Textual silently swaps the mounted table or fence for it.

That half is transient. The second half makes it **permanent**: `TranscriptPane._prepare_committed_entry`'s
tail-to-entry handoff only re-writes the widget (`EntryMarkdown.update`, which *does* self-correct
an already-corrupted widget when it is actually called — proven positively in
`test_a_cleanly_completed_multi_delta_table_keeps_its_live_tails_corruption`) when the committed
text differs from what the live tail last had applied. A table that completes cleanly via
`message.complete` reports exactly the text that was already streamed, so `record.raw_body ==
tail.applied_text`, the corrective write is skipped as a no-op, and the corrupted widget is reused
verbatim — forever, in the committed, settled transcript. A cancelled or disconnected turn is
accidentally immune: its commit always appends an interruption marker, so the text always differs
from what the tail last applied and the corrective `update()` always runs — which is exactly why
`test_early_termination_by_cancel_and_disconnect_commits_partial_content` (the AE2 scenarios) is
unaffected by this defect and passes clean.

**Reproducibility depends on render cadence.** This only manifests when the coalescing render tick
catches the construct mid-growth more than once before it completes. A real live session, with a
gateway streaming genuine wall-clock-timed deltas across a 50ms coalescing boundary (KTD3), makes
that likely for any construct spanning more than one delta. A single-drain unbounded replay that
lets an entire small corpus land before the render timer ever fires once does *not* reproduce it —
the tail is constructed directly with its already-final text and never takes the corrupting append
path at all. `test_a_cleanly_completed_multi_delta_table_keeps_its_live_tails_corruption` drives the
replay frame by frame, rendering after each one, to reproduce it reliably; the docstring on that
test states this dependency explicitly so a future reader does not mistake an unbounded-replay pass
for the defect being fixed.

**Consequence for this gate's verdict.** `feature_corpus_content_loss` fails in every `run_gate()`
run in this pass, and it is expected to keep failing until `talaria/ui/` is fixed. This is a real
R1/R5 gap — a table that streams progressively and completes normally can render its final,
committed, on-screen form incorrectly — reported plainly rather than dressed as green. It does not
affect `content_is_complete` (the domain-side proof, R11a, untouched and still exact) — the
**domain** text is always correct; only the **mounted document's** internal structure is wrong,
which is precisely the class of defect the ownership proof (not the older, weaker line-window
claim) exists to see.

## Environment this was verified against

- Textual `8.2.8`, Python `3.12.11`, `Darwin 25.5.0`/`arm64`, commit `921fe0d7ead838411fb8c6f357dd89a723be786a`
  on `feat/v0-2-block-markdown-build` (`talaria.replay.gate.build_matrix()`).
- Terminal geometry: `GATE_SIZE = (100, 40)` (gate.py's existing pin, unchanged); the mega-line
  workload additionally exercises an 80-column resize mid-growth.
- Theme: `textual-dark` (the app's existing default; not overridden by this unit's work).
- Corpus identities: `talaria-feature-v1`, sha256
  `de9d6f55d54e166c559dc3c7228fcfb665ef6d5cdec042b7648716de5465e12a`, 47 frames, scripted
  (deterministic by construction, no seed). The stress corpus's own identity is unchanged in
  mechanism (`talaria-stress-v1-{deltas}d-seed{seed}`) — re-running it against current code is
  itself the U2 re-baseline; no digest is published here for a specific delta count because no
  full-scale run was executed in this pass (see "What remains" below).

## Reproduction

```bash
# The R11a guard: the v0.1 pin, verbatim.
uv run pytest tests/domain/test_projection.py -q                    # 14 passed

# Every U6 test, this revision's additions included.
uv run pytest tests/replay/test_gate.py -q                          # 60 passed (30 new since the
                                                                      # prior partial revision)
uv run pytest tests/replay -q                                       # test_gate.py, test_controls.py,
                                                                      # test_operator_line.py,
                                                                      # test_source.py together

# The full-size boundary probes and the exact-boundary regression, standalone:
uv run pytest tests/replay/test_gate.py -k "601_column or 599_column or one_column_table_workload" -q

# The fence-oracle correctness fix:
uv run pytest tests/replay/test_gate.py::test_an_unclosed_fence_with_real_trailing_content_is_not_a_false_positive -q

# The discovered defect, isolated and reproduced:
uv run pytest tests/replay/test_gate.py::test_a_cleanly_completed_multi_delta_table_keeps_its_live_tails_corruption -q

# Checks
uv run ruff check talaria/replay tests/replay                       # clean
uv run mypy talaria/replay tests/replay                             # clean
uv run bandit -r talaria -q                                         # 1 low finding: the single
  # pre-existing baseline assert in talaria/ui/transcript.py:514 (_MountedUnit.widgets). The fix
  # round's two new asserts in gate.py's block_documents_are_owned, and one this pass added in
  # _apply_sideband_action, are all now explicit narrowing (an `if x is None:` branch that reports
  # a proof failure or raises, never a bare assert in talaria/ production code) -- reported back to
  # exactly the accepted baseline.
```

## What remains — stated plainly, not silently absorbed

- **A full-scale KTD14 gate run has not been executed in this pass.** `run_gate()`'s default
  `deltas=50_000` stress-corpus pass, its sustained-cadence pass, and a full-size
  `run_adversarial_workloads()` invocation (the fence to 10,000 lines, the table to 1,000 rows, the
  mega-line to 100,000 characters, run together rather than individually as the standalone tests
  above do) have not been run to completion and published with a `verdict: pass`/`fail` and
  corpus-sha256 citation in this pass. Every number in this document is either a reduced-scale test
  run or a direct, full-size invocation of one specific measurement function — real evidence, but
  not the KTD14-scale claim this document's lineage (`2026-08-03-textual-validation-gate-results.md`)
  set as precedent. `uv run python -c "import asyncio; from talaria.replay.gate import run_gate;
  print(asyncio.run(run_gate()).to_dict())"` is the exact command to run next; expect it to take
  several minutes and to fail on `feature_corpus_content_loss` for the reason stated above.
- **The discovered defect is not fixed.** It lives in `talaria/ui/`, out of this unit's scope
  (explicit instruction: report what and why rather than edit). `feature_corpus_content_loss` will
  keep failing until it is.
- **Tier-one descendant high-water figures under the KTD1(d) growth workloads specifically** are not
  separately published — the workload harness grows a single live tail with no folded window, so
  tier one (the folded window's own 600-descendant ceiling) is proven by the existing 302-entry
  aggregate-ceiling and odd-cut/partial-retention regressions from the prior pass, not by anything
  new in this revision.

The honest summary: all four gaps are built, mechanically correct, and tested — including one
genuine correctness fix inside the ownership proof itself (the fence oracle) and one genuine defect
this work found in code outside its scope (the table/fence append-corruption) rather than hid. What
is outstanding is scale, not mechanism: a full KTD14-sized run has not been executed, and the
`talaria/ui/` defect this pass found has not been fixed. Both are named here so the next pass starts
from an accurate map.
