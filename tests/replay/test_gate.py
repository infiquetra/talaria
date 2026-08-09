"""The gate harness itself: does the instrument work, and can it fail?

A measurement harness nobody has seen fail is a harness that might not measure
anything. Every check below either proves a measurement responds to the thing it
claims to measure, or proves the verdict arithmetic turns a breached threshold
into a ``fail`` — because the plan makes a fail verdict *halt the run*, and a
gate that cannot produce one is worse than no gate.

The gate is run here at a fraction of KTD14's size. The full-size run is not a
test; it is the recorded measurement in
``docs/analysis/2026-08-03-textual-validation-gate-results.md``.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

import pytest
from textual.app import App, ComposeResult
from textual.geometry import Size

from talaria.domain.models import TranscriptEntry
from talaria.domain.projection import (
    EntryScopedView,
    ProvisionalTail,
    TranscriptEntryRecord,
    TranscriptView,
)
from talaria.domain.state import SessionState
from talaria.replay import gate as gate_module
from talaria.replay.gate import (
    DESCENDANT_WIDGET_CEILING,
    CorpusIdentity,
    GateMeasurement,
    GateResult,
    _cadence_speed,
    _expected_top_level_blocks,
    _fit_slope,
    _prove_span_coverage,
    block_documents_are_owned,
    content_is_complete,
    document_ownership,
    expected_block_documents_are_mounted,
    fallback_banner_accounting,
    measure_replay,
    ownership_report,
    run_gate,
    stress_corpus_identity,
)
from talaria.replay.stress import build_stress_corpus
from talaria.ui.blocks import EntryMarkdown
from talaria.ui.transcript import TranscriptPane


def _identity() -> CorpusIdentity:
    return CorpusIdentity(label="t", sha256="0" * 64, frame_count=0, kind="test")


def _measurement(**overrides: Any) -> GateMeasurement:
    base: dict[str, Any] = {
        "corpus": _identity(),
        "frames_applied": 1,
        "render_ticks": 1,
        "elapsed_seconds": 1.0,
        "render_ticks_per_second": 1.0,
        "peak_mounted_widgets": 1,
        "condensed_lines": 0,
    }
    base.update(overrides)
    return GateMeasurement(**base)


# ── the measurements respond to what they measure ────────────────────────


def test_content_completeness_detects_a_dropped_entry() -> None:
    state = SessionState(
        transcript=(
            TranscriptEntry(kind="assistant", text="first", turn_index=0, seq=1),
            TranscriptEntry(kind="assistant", text="second", turn_index=0, seq=2),
        )
    )
    complete = TranscriptView(lines=("first", "second"), entry_count=2)
    lossy = TranscriptView(lines=("first",), entry_count=1)
    reordered = TranscriptView(lines=("second", "first"), entry_count=2)

    assert content_is_complete(state, complete) is True
    assert content_is_complete(state, lossy) is False
    assert content_is_complete(state, reordered) is False


def test_content_completeness_matches_whole_lines_not_substrings() -> None:
    """Substring containment let a corrupted line pass as an intact one.

    Every rendered line still *contains* the original text, so a renderer that
    appended to each one satisfied the old check while showing the operator
    something the domain never said.
    """
    state = SessionState(
        transcript=(TranscriptEntry(kind="assistant", text="first", turn_index=0, seq=1),)
    )

    assert content_is_complete(state, TranscriptView(lines=("first",), entry_count=1)) is True
    assert (
        content_is_complete(state, TranscriptView(lines=("first + junk",), entry_count=1)) is False
    )


def test_content_completeness_does_not_let_an_empty_entry_match_anything() -> None:
    """An empty entry produced an empty fragment, and ``"" in anything`` is True.

    So an empty-text entry matched whatever line the cursor happened to sit on
    and advanced past it, consuming a real line's slot without checking it.
    """
    state = SessionState(
        transcript=(
            TranscriptEntry(kind="assistant", text="", turn_index=0, seq=1),
            TranscriptEntry(kind="assistant", text="third", turn_index=0, seq=2),
        )
    )

    # The empty entry renders as an empty line; both must be present, in order.
    assert content_is_complete(state, TranscriptView(lines=("", "third"), entry_count=2)) is True

    # The discriminating case. Here the empty entry's line is absent and an
    # unrelated line sits in its place. Under substring matching the empty
    # fragment matched "second", consumed that slot, and "third" then matched
    # the next line -- so the whole thing passed while the projection showed a
    # line the domain never committed and omitted one it did. Chosen so the
    # cursor does not simply run out of lines, which is a different failure and
    # would have made this test pass against the unfixed code too.
    assert (
        content_is_complete(state, TranscriptView(lines=("second", "third"), entry_count=2))
        is False
    )


def test_the_memory_slope_is_a_fit_over_the_series_not_the_endpoints() -> None:
    flat = [(0, 50.0), (5000, 50.0), (10000, 50.0)]
    rising = [(0, 50.0), (5000, 55.0), (10000, 60.0)]
    assert _fit_slope(flat) == pytest.approx(0.0)
    assert _fit_slope(rising) == pytest.approx(1.0)
    assert _fit_slope([(0, 50.0)]) == 0.0


def test_the_cadence_speed_scales_with_the_corpus_duration() -> None:
    short = build_stress_corpus(deltas=200).records
    long_corpus = build_stress_corpus(deltas=20_000).records
    assert _cadence_speed(short) < _cadence_speed(long_corpus)
    assert _cadence_speed(()) == 1.0


# ── the verdict arithmetic ───────────────────────────────────────────────


def test_a_breached_threshold_produces_a_fail_verdict() -> None:
    result = GateResult(
        stress=_measurement(),
        cadence=_measurement(),
        live=None,
        determinism_identical=True,
        inert_controls_refused=(),
        matrix={},
        checks={
            "ok": {"measured": 1, "threshold": 2, "comparison": "<=", "pass": True},
            "breached": {"measured": 900, "threshold": 600, "comparison": "<=", "pass": False},
        },
    )
    assert result.passed is False
    assert result.verdict == "fail"
    assert result.to_dict()["verdict"] == "fail"


def test_all_checks_passing_produces_a_pass_verdict() -> None:
    result = GateResult(
        stress=_measurement(),
        cadence=_measurement(),
        live=None,
        determinism_identical=True,
        inert_controls_refused=(),
        matrix={},
        checks={"ok": {"measured": 1, "threshold": 2, "comparison": "<=", "pass": True}},
    )
    assert result.passed is True
    assert result.verdict == "pass"


# ── an end-to-end run at reduced size ────────────────────────────────────


@pytest.mark.asyncio
async def test_the_gate_runs_end_to_end_and_records_corpus_identity() -> None:
    """The harness works: it runs, measures, and cites its corpus opaquely.

    Deliberately does not assert the verdict. The verdict is a claim about the
    *product*, and it is currently ``fail`` — see the strict xfail below, which
    is the honest place to record that.
    """
    result = await run_gate(live_corpus=None, deltas=600, seed=7)
    assert result.stress.corpus.kind == "synthetic-stress"
    assert len(result.stress.corpus.sha256) == 64
    assert result.stress.corpus.frame_count > 600
    assert result.stress.frames_applied == result.stress.corpus.frame_count
    assert result.matrix["textual"].startswith("8.")
    # The identity is opaque: nothing that could be a local path.
    assert "/" not in result.stress.corpus.label


#: The two checks that are floors on *how much was sampled*, not claims about the
#: product. Both ride a 20ms poll against ``RSS_SAMPLE_EVERY`` frames, so they can
#: only be satisfied by a run long enough in wall-clock terms to be polled that
#: many times — which a 600-delta corpus at unbounded replay speed never is; it
#: drains between two polls. They are calibrated for KTD14's real 50,000-delta
#: run and are reported by the published evidence, not by this test.
SCALE_DEPENDENT_CHECKS = frozenset({"enough_memory_samples", "enough_content_checkpoints"})


@pytest.mark.asyncio
async def test_every_product_claim_holds_at_reduced_scale() -> None:
    """The product claims, kept separate so a failure is visible rather than absorbed.

    This used to assert ``verdict == "pass"`` under a strict xfail blamed entirely
    on ``TranscriptPane``'s reconciliation. Half of that was right — the pane did
    desynchronize, and ``tests/ui/test_transcript_bounds.py`` now pins the fix at
    unit size — but the attribution was wrong: at 600 deltas the two sample-count
    floors above cannot be met by any implementation, so the assertion was
    unreachable on its own terms and would have stayed red after a perfect fix.
    An xfail whose stated reason is not the only reason is a comment that lies.

    What is asserted instead is exact and reachable: every check that is a claim
    about the product passes, and the only ones that do not are the two scale
    floors, named. A content-loss regression puts ``content_loss`` in the failing
    set, which is not a subset of the named floors, so this still turns red.

    The original gate could not detect a rendering defect at all: its content
    check compared ``transcript_view(app.state)`` against ``app.state`` — the
    projection against itself — so no interface defect of any kind could fail it.
    """
    result = await run_gate(live_corpus=None, deltas=600, seed=7)
    failed = {name for name, check in result.checks.items() if not check["pass"]}
    assert failed <= SCALE_DEPENDENT_CHECKS, f"a product claim failed: {sorted(failed)}"
    assert result.stress.content_loss_failures == 0
    assert result.cadence.content_loss_failures == 0
    # The scale floors are named rather than skipped, so nobody reads the
    # subset assertion above as "the gate passed at this size". It did not.
    assert result.verdict == "fail"
    assert failed == SCALE_DEPENDENT_CHECKS


@pytest.mark.asyncio
async def test_the_stress_corpus_is_reproducible_from_its_seed() -> None:
    first = build_stress_corpus(deltas=400, seed=11)
    second = build_stress_corpus(deltas=400, seed=11)
    different = build_stress_corpus(deltas=400, seed=12)
    assert first.sha256 == second.sha256
    assert first.sha256 != different.sha256
    assert first.label == second.label


@pytest.mark.asyncio
async def test_a_lowered_ceiling_makes_the_gate_fail(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The gate is demonstrably capable of failing, not merely of passing."""
    monkeypatch.setattr(gate_module, "MOUNTED_WIDGET_CEILING", 1)
    result = await run_gate(live_corpus=None, deltas=400, seed=3)
    assert result.verdict == "fail"
    assert result.checks["mounted_widgets"]["pass"] is False


@pytest.mark.asyncio
async def test_measuring_a_replay_reports_the_frames_it_actually_applied() -> None:
    corpus = build_stress_corpus(deltas=300, seed=5)
    measurement, state, refusals = await measure_replay(
        corpus.records, stress_corpus_identity(corpus)
    )
    assert measurement.frames_applied == corpus.frame_count
    assert measurement.transcript_entries > 0
    assert measurement.rss_series_mb[0][0] == 0
    assert refusals == ()
    # The malformed frames in the corpus were surfaced, not skipped.
    assert state.protocol_error_count > 0
    assert state.unknown_event_types


# ── U6: the two-part ownership proof ──────────────────────────────────────
#
# `interface_shows_everything`'s original claim was one-line-one-widget,
# true only while every mounted unit was a `TranscriptLine`. U4 broke that
# premise for committed assistant/reasoning entries, which mount as one
# `EntryMarkdown` document instead. These tests prove the two-part
# ownership proof that replaced it: (1) mounted-window ownership — a block
# document's own top-level blocks, read through Textual's public
# `source_range`, genuinely cover the lines they claim to, with the
# blank-line accounting rule from the probed fact stated in KTD8's branch
# instruction; (2) condensed-range accounting, including RA3's fallback
# banner charge. Every "the proof holds" test here is paired with at least
# one "and it can be made to fail" mutation test — a proof nobody has seen
# fail is a proof that might not be proving anything (the same reasoning
# `test_gate.py`'s own module docstring states for the gate as a whole).


class _DocHarness(App[None]):
    """Mounts one EntryMarkdown so the ownership proof can be probed
    directly, without a full TalariaApp/domain replay."""

    def __init__(self, text: str) -> None:
        super().__init__()
        self._text = text

    def compose(self) -> ComposeResult:
        yield EntryMarkdown(self._text)


@asynccontextmanager
async def mounted_document(text: str) -> AsyncIterator[EntryMarkdown]:
    app = _DocHarness(text)
    async with app.run_test() as pilot:
        await pilot.pause()
        yield app.query_one(EntryMarkdown)


# ── the blank-line accounting rule, stated and tested ─────────────────────


@pytest.mark.asyncio
async def test_the_blank_line_probe_is_exactly_what_the_branch_instruction_states() -> None:
    """``"a\\n\\n# h\\n"`` yields ranges (0, 1) and (2, 3); line 1, the blank
    between them, sits inside neither span. The exact probed fact this
    unit's KTD8 branch-hold instruction names — pinned here as ground truth
    for :func:`document_ownership`'s blank-line accounting rule, not merely
    asserted in a docstring.
    """
    from textual.widgets._markdown import MarkdownBlock as _Block

    async with mounted_document("a\n\n# h\n") as widget:
        blocks = [c for c in widget.children if isinstance(c, _Block)]
        spans = [(type(c).__name__, c.source_range) for c in blocks]
        assert spans == [("MarkdownParagraph", (0, 1)), ("MarkdownH1", (2, 3))]
        ok, reason = document_ownership(widget)
        assert ok, reason


def test_prove_span_coverage_accounts_the_inter_block_blank_to_the_entry_not_a_block() -> None:
    """The pure half of the rule, isolated from any live Textual widget: a
    blank line between two covered spans passes; the same blank line
    reclassified as non-blank text (nothing rendered it) fails.
    """
    lines = ["a", "", "# h"]
    spans: list[tuple[str, tuple[int, int]]] = [
        ("MarkdownParagraph", (0, 1)),
        ("MarkdownH1", (2, 3)),
    ]
    ok, reason = _prove_span_coverage(lines, spans)
    assert ok, reason

    # The same shape, but the "blank" line actually carries text no block
    # claims -- the accounting rule must not wave this through.
    ok, reason = _prove_span_coverage(["a", "not blank", "# h"], spans)
    assert ok is False
    assert "owned by no block" in reason


def test_prove_span_coverage_treats_a_whitespace_only_inter_block_line_as_blank_too() -> None:
    """CR6's confirmed finding at gate.py:452, the reviewer's own live REPL
    reproduction, pinned as a test: before the fix, checking only the
    literal ``""`` rejected ``"  "`` — a whitespace-only inter-block line —
    as "dropped text", even though the markdown parser itself treats a
    space-only line exactly as it treats an empty one (neither ever opens a
    construct, so neither is ever claimed by a block's ``source_range``).
    """
    lines = ["a", "  ", "# h"]
    spans: list[tuple[str, tuple[int, int]]] = [
        ("MarkdownParagraph", (0, 1)),
        ("MarkdownH1", (2, 3)),
    ]
    ok, reason = _prove_span_coverage(lines, spans)
    assert ok, reason


@pytest.mark.asyncio
async def test_a_whitespace_only_inter_block_line_does_not_break_document_ownership() -> None:
    """The same reproduction, driven through the real parser and a real
    mounted document rather than a hand-built spans list — confirms the
    parser genuinely treats ``"a\\n  \\n# h\\n"``'s middle line as blank
    (it opens no construct, so no block's own ``source_range`` ever claims
    it), which is the fact the fixed accounting rule now relies on.
    """
    async with mounted_document("a\n  \n# h\n") as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason


# ── mutation tests: the proof can fail ─────────────────────────────────────


def test_dropped_text_makes_span_coverage_fail() -> None:
    """A block whose span shrank so it no longer covers its own non-blank
    content — the "dropped text" mutation named in KTD8's branch
    instruction, tested at the pure layer directly.
    """
    lines = ["first line", "second line"]
    healthy: list[tuple[str, tuple[int, int]]] = [("MarkdownParagraph", (0, 2))]
    ok, reason = _prove_span_coverage(lines, healthy)
    assert ok, reason

    dropped: list[tuple[str, tuple[int, int]]] = [("MarkdownParagraph", (0, 1))]
    ok, reason = _prove_span_coverage(lines, dropped)
    assert ok is False
    assert "second line" in reason


def test_a_hidden_construct_makes_span_coverage_fail_by_being_absent_entirely() -> None:
    """A construct dropped from the mounted sequence altogether — no span at
    all claims its lines — fails the same way as a shrunk span.
    """
    lines = ["heading text", "", "paragraph text"]
    only_one: list[tuple[str, tuple[int, int]]] = [("MarkdownH1", (0, 1))]
    ok, reason = _prove_span_coverage(lines, only_one)
    assert ok is False
    assert "paragraph text" in reason


@pytest.mark.asyncio
async def test_a_wrong_block_class_makes_document_ownership_fail() -> None:
    """Span coverage alone cannot see a block mounted as the wrong
    construct, since two adjacent same-length spans can still cover every
    line correctly. The semantic oracle's class check is what catches it —
    proven here by reassigning a real mounted paragraph's class to the
    heading class next to it (source_range and text left untouched).
    """
    async with mounted_document("first para\n\n# h\n") as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason

        paragraph, heading = widget.children[0], widget.children[1]
        assert type(paragraph).__name__ == "MarkdownParagraph"
        paragraph.__class__ = type(heading)

        ok, reason = document_ownership(widget)
        assert ok is False
        assert "wrong block class" in reason


@pytest.mark.asyncio
async def test_document_ownership_is_self_referential_without_applied_text() -> None:
    """The bug named at gate.py:463 (CR6), pinned as ground truth before the
    fix below is proven: ``document_ownership(widget)`` with no
    ``applied_text`` checks only that the document's own blocks cover its
    own ``.source`` -- so replacing that ``.source`` wholesale, with
    different but still internally-consistent markdown, passes. This is not
    the desired behaviour; it is the false pass CR6 found, kept as a named
    fact so the next test's fix is legible as a fix.
    """
    async with mounted_document("# original\n") as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason

        await widget.update("# substituted")

        ok, reason = document_ownership(widget)
        assert ok, "a swap to internally-consistent text is invisible without applied_text"


@pytest.mark.asyncio
async def test_document_ownership_catches_a_document_swapped_for_consistent_text() -> None:
    """CR6's confirmed finding, fixed: ``document_ownership`` must also
    compare ``widget.source`` against the domain's actual applied text for
    that document, not only against the document's own blocks. A document
    whose ``.source`` was swapped for different — but still internally
    self-consistent — markdown (every span still covers it, every construct
    oracle still holds) now fails, because the swap is caught against what
    the document was supposed to still be showing.
    """
    async with mounted_document("# original") as widget:
        ok, reason = document_ownership(widget, applied_text="# original")
        assert ok, reason

        await widget.update("# substituted")

        ok, reason = document_ownership(widget, applied_text="# original")
        assert ok is False
        assert "does not match the document's own applied text" in reason


@pytest.mark.asyncio
async def test_block_documents_are_owned_catches_a_real_mounted_entry_swapped_for_wrong_text() -> (
    None
):
    """The same swap, proven through :func:`block_documents_are_owned`
    against a real :class:`~talaria.ui.transcript.TranscriptPane` rather
    than a bare widget — the shape CR6's context citation
    (``talaria/ui/transcript.py:584``) points at: the pane's own
    ``rendered_lines``/``_MountedUnit.applied_text`` bookkeeping is what the
    swapped widget is checked against.
    """

    class _PaneHarness(App[None]):
        def compose(self) -> ComposeResult:
            yield TranscriptPane(id="t")

    committed = TranscriptEntryRecord(
        entry_id=1,
        kind="assistant",
        raw_body="# original heading\n",
        committed=True,
        line_span=(0, 1),
    )
    entries = EntryScopedView(
        entries=(committed,),
        assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=0),
        reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="", generation=0),
    )
    view = TranscriptView(lines=("# original heading",), entry_count=1)

    app = _PaneHarness()
    async with app.run_test() as pilot:
        pane = app.query_one(TranscriptPane)
        await pane.apply(view, entries)
        await pilot.pause()

        ok, reason = block_documents_are_owned(pane)
        assert ok, reason

        # The swap: the widget's own source changes to different,
        # internally-consistent text, while the pane's own bookkeeping
        # (`_MountedUnit.applied_text`) is left saying the original text was
        # last applied -- exactly a document silently substituted after
        # being built.
        unit = pane._entries[1]
        assert unit.block is not None
        await unit.block.update("# substituted heading")

        ok, reason = block_documents_are_owned(pane)
        assert ok is False
        assert "entry 1" in reason
        assert "does not match the document's own applied text" in reason


@pytest.mark.asyncio
async def test_a_hidden_construct_inside_a_fence_makes_the_fence_oracle_fail() -> None:
    """Span coverage and the class check both pass for a fence whose
    *content* was silently swapped or dropped after mounting — Textual's
    ``MarkdownFence.code`` is the exact code text the fence widget actually
    holds, so corrupting it directly is the "hidden construct" mutation:
    the right class, the right span, the wrong (or missing) content.
    """
    from textual.widgets._markdown import MarkdownFence as _Fence

    async with mounted_document("```py\nreal code\n```\n") as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason

        fence = widget.children[0]
        assert isinstance(fence, _Fence)
        original = fence.code
        fence.code = "substituted, not the real fence body"

        ok, reason = document_ownership(widget)
        assert ok is False
        assert "does not match source body" in reason

        fence.code = ""
        ok, reason = document_ownership(widget)
        assert ok is False

        fence.code = original
        ok, reason = document_ownership(widget)
        assert ok, reason


@pytest.mark.asyncio
async def test_a_dropped_table_cell_makes_the_table_grid_oracle_fail() -> None:
    """R1's table construct, proven exact: Textual mounts one widget per
    cell (header and body alike), so removing one from the live tree is a
    hidden/dropped cell the span-coverage check cannot see (the table's own
    top-level span is untouched) but the cell-count oracle can.
    """
    from textual.widgets._markdown import MarkdownTableCellContents

    async with mounted_document("| a | b |\n| - | - |\n| 1 | 2 |\n") as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason

        table = widget.children[0]
        cells = list(table.query(MarkdownTableCellContents))
        assert len(cells) == 4  # 2 header + 2 body, exact
        await cells[0].remove()

        ok, reason = document_ownership(widget)
        assert ok is False
        assert "dropped or hidden cell" in reason


@pytest.mark.asyncio
async def test_a_dropped_list_item_makes_the_list_oracle_fail() -> None:
    """Both list types (R1) mount one ``Horizontal`` row per item (probed:
    Textual's own ``MarkdownBulletList.compose``/``MarkdownOrderedList
    .compose``) — removing one row is a dropped item the span-coverage
    check cannot see, since the list's own top-level span is untouched.
    """
    from textual.containers import Horizontal

    async with mounted_document("- a\n- b\n- c\n") as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason

        the_list = widget.children[0]
        rows = list(the_list.query(Horizontal))
        assert len(rows) == 3
        await rows[0].remove()

        ok, reason = document_ownership(widget)
        assert ok is False
        assert "dropped or hidden item" in reason


@pytest.mark.asyncio
async def test_a_flattened_quote_makes_the_blockquote_oracle_fail() -> None:
    """A block quote whose nested content was silently flattened away —
    mounted as the right class, spanning the right lines, but empty
    underneath. Constructed by removing the quote's only nested child.
    """
    async with mounted_document("> quoted text\n") as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason

        from textual.widget import Widget as _Widget

        quote = widget.children[0]
        assert type(quote).__name__ == "MarkdownBlockQuote"
        nested = [c for c in quote.walk_children() if isinstance(c, _Widget)]
        assert nested, "the fixture must have nested content to strip for this to be a test"
        for child in nested:
            await child.remove()

        ok, reason = document_ownership(widget)
        assert ok is False
        assert "hidden construct" in reason


# ── per-construct oracle: every R1 construct proven positively too ────────


@pytest.mark.asyncio
async def test_document_ownership_holds_across_every_r1_construct_at_once() -> None:
    """Heading, both list types, table, fence, and block quote in one
    document — the positive case every mutation test above is a negative
    twin of.
    """
    source = (
        "# heading\n\n"
        "a paragraph\n\n"
        "> a quote\n\n"
        "- bullet one\n- bullet two\n\n"
        "1. ordered one\n2. ordered two\n\n"
        "| a | b |\n| - | - |\n| 1 | 2 |\n\n"
        "```py\ncode body\n```\n"
    )
    async with mounted_document(source) as widget:
        ok, reason = document_ownership(widget)
        assert ok, reason
        classes = [type(c).__name__ for c in widget.children]
        assert classes == [
            "MarkdownH1",
            "MarkdownParagraph",
            "MarkdownBlockQuote",
            "MarkdownBulletList",
            "MarkdownOrderedList",
            "MarkdownTable",
            "MarkdownFence",
        ]


# ── zero-block sources fall under the window comparison, not the proof ────


def test_expected_top_level_blocks_is_empty_for_a_zero_block_source() -> None:
    """An empty/whitespace/newline-only source parses to zero top-level
    blocks — :func:`block_documents_are_owned` never has to special-case
    this, because :class:`~talaria.ui.transcript.TranscriptPane` never
    mounts an ``EntryMarkdown`` for one (KTD2's ``is_zero_block``); it stays
    line-rendered and is proven under the retained window comparison.
    """
    for blank in ("", "\n", "   \n\n  \n", "\n\n\n"):
        assert _expected_top_level_blocks(blank) == []


@pytest.mark.asyncio
async def test_block_documents_are_owned_over_committed_entries_and_both_live_tails() -> None:
    """"applies to both provisional tail documents at every checkpoint, not
    only committed entries" — driven end to end through the real
    :class:`~talaria.ui.transcript.TranscriptPane`, not just the per-widget
    proof, so the pane actually mounting a tail's document is exercised
    too, not only the shape of a hand-built widget.
    """
    from talaria.domain.projection import EntryScopedView, ProvisionalTail, TranscriptEntryRecord
    from talaria.ui.transcript import TranscriptPane

    class _PaneHarness(App[None]):
        def compose(self) -> ComposeResult:
            yield TranscriptPane(id="t")

    committed = TranscriptEntryRecord(
        entry_id=1,
        kind="assistant",
        raw_body="# committed heading\n",
        committed=True,
        line_span=(0, 1),
    )
    entries = EntryScopedView(
        entries=(committed,),
        assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=0),
        reasoning_tail=ProvisionalTail(
            kind="reasoning", raw_text="## live reasoning tail\n", generation=1
        ),
    )
    view = TranscriptView(
        lines=("# committed heading", "· ## live reasoning tail"),
        entry_count=1,
    )

    app = _PaneHarness()
    async with app.run_test() as pilot:
        pane = app.query_one(TranscriptPane)
        await pane.apply(view, entries)
        await pilot.pause()

        documents = list(pane.query(EntryMarkdown))
        assert len(documents) == 2, "the committed entry and the reasoning tail must both mount"

        ok, reason = block_documents_are_owned(pane)
        assert ok, reason


@pytest.mark.asyncio
async def test_a_zero_block_entry_line_renders_and_mounts_no_entry_markdown() -> None:
    from talaria.domain.projection import EntryScopedView, ProvisionalTail, TranscriptEntryRecord
    from talaria.ui.transcript import TranscriptPane

    class _PaneHarness(App[None]):
        def compose(self) -> ComposeResult:
            yield TranscriptPane(id="t")

    blank_entry = TranscriptEntryRecord(
        entry_id=1,
        kind="assistant",
        raw_body="\n\n",
        committed=True,
        line_span=(0, 3),
    )
    entries = EntryScopedView(
        entries=(blank_entry,),
        assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=0),
        reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="", generation=0),
    )
    view = TranscriptView(lines=("", "", ""), entry_count=1)

    app = _PaneHarness()
    async with app.run_test() as pilot:
        pane = app.query_one(TranscriptPane)
        await pane.apply(view, entries)
        await pilot.pause()

        assert list(pane.query(EntryMarkdown)) == []
        ok, reason = block_documents_are_owned(pane)
        assert ok, reason  # vacuously true -- nothing to prove
        assert pane.rendered_lines == ("", "", "")


@pytest.mark.asyncio
async def test_a_tail_collapsing_to_a_bare_newline_stays_mounted_as_a_block_and_is_rejected() -> (
    None
):
    """CR6: "zero-block sources are never reached here at all" was asserted
    in :func:`block_documents_are_owned`'s own docstring, never verified.
    Driven live against the real pane: a reasoning tail grows into a block
    document, then collapses to a bare newline as a new generation (a
    replace-wins reset, not a plain append) -- and
    :meth:`TranscriptPane._reconcile_tail`'s block-kind branch only
    re-checks ``trips_fallback_trigger`` after writing, never
    ``is_zero_block``, so the now-empty tail stays mounted as a height-zero
    ``EntryMarkdown`` rather than being demoted to line rendering. Before
    this unit's fix, ``document_ownership`` passed a document like this
    vacuously (zero blocks trivially "cover" a zero-length span); it must
    now reject it outright.
    """
    from talaria.domain.projection import EntryScopedView, ProvisionalTail
    from talaria.ui.transcript import TranscriptPane

    class _PaneHarness(App[None]):
        def compose(self) -> ComposeResult:
            yield TranscriptPane(id="t")

    app = _PaneHarness()
    async with app.run_test() as pilot:
        pane = app.query_one(TranscriptPane)

        grown = EntryScopedView(
            entries=(),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=0),
            reasoning_tail=ProvisionalTail(
                kind="reasoning", raw_text="# heading\n\nreal content", generation=0
            ),
        )
        await pane.apply(
            TranscriptView(lines=("# heading", "", "real content"), entry_count=0), grown
        )
        await pilot.pause()
        assert pane._tails["reasoning"] is not None
        assert pane._tails["reasoning"].kind == "block"

        collapsed = EntryScopedView(
            entries=(),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=0),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="\n", generation=1),
        )
        await pane.apply(TranscriptView(lines=("",), entry_count=0), collapsed)
        await pilot.pause()

        tail_unit = pane._tails["reasoning"]
        assert tail_unit is not None
        assert tail_unit.kind == "block", "the live bug this test pins: it never got demoted"
        assert tail_unit.block is not None
        assert tail_unit.block.outer_size.height == 0, "a zero-block document mounts at height 0"

        ok, reason = block_documents_are_owned(pane)
        assert ok is False
        assert "zero top-level blocks" in reason


# ── RA3: the fallback banner proof ─────────────────────────────────────────


class _FakeClassed:
    def __init__(self, *classes: str, height: int = 1) -> None:
        self.classes = frozenset(classes)
        #: `fallback_banner_accounting` reads `.outer_size.height` on a
        #: banner widget (the row-count measurement, CR6); a plain fake with
        #: no such attribute would raise instead of exercising the check.
        self.outer_size = Size(1, height)


class _FakePane:
    """Duck-typed stand-in for the two things `fallback_banner_accounting`
    reads: ``.children``, an ordered sequence of objects with ``.classes``
    and ``.outer_size``.
    """

    def __init__(self, children: list[_FakeClassed]) -> None:
        self.children = children


def test_fallback_banner_accounting_passes_for_a_run_with_its_banner() -> None:
    pane = _FakePane(
        [
            _FakeClassed("transcript--nowrap"),
            _FakeClassed("transcript--nowrap"),
            _FakeClassed("transcript--fallback-banner"),
        ]
    )
    ok, reason = fallback_banner_accounting(pane)  # type: ignore[arg-type]
    assert ok, reason


def test_fallback_banner_accounting_fails_when_the_banner_is_missing() -> None:
    """A fallen-back run with no banner after it -- the fold rule's own
    "a banner never stands alone" clause, inverted: content standing
    without its charge.
    """
    pane = _FakePane([_FakeClassed("transcript--nowrap"), _FakeClassed("transcript--nowrap")])
    ok, reason = fallback_banner_accounting(pane)  # type: ignore[arg-type]
    assert ok is False
    assert "no banner" in reason


def test_fallback_banner_accounting_fails_when_a_banner_stands_alone() -> None:
    """A banner with no fallback content run in front of it -- charged to
    nothing, which is exactly the accounting RA3 forbids.
    """
    pane = _FakePane([_FakeClassed("transcript--fallback-banner")])
    ok, reason = fallback_banner_accounting(pane)  # type: ignore[arg-type]
    assert ok is False
    assert "standing alone" in reason


def test_fallback_banner_accounting_fails_when_the_banner_wraps_past_one_row() -> None:
    """CR6: the "+1" in KTD1(a)'s exact-row formula is a claim about a
    *painted row*, not a mounted widget. A banner whose own text wraps to
    more than one row breaches that formula even though the DOM shape (one
    run, one banner, immediately adjacent) is otherwise exactly right.
    """
    pane = _FakePane(
        [
            _FakeClassed("transcript--nowrap"),
            _FakeClassed("transcript--fallback-banner", height=2),
        ]
    )
    ok, reason = fallback_banner_accounting(pane)  # type: ignore[arg-type]
    assert ok is False
    assert "2 rows" in reason


# ── RA3 against real fold arithmetic, not class-only fakes (CR6) ──────────
#
# The tests above pin `fallback_banner_accounting`'s own DOM-walking logic in
# isolation, cheaply. They cannot fail on a wrong fold count or a wrong
# odd-cut arm, because nothing about a hand-built `_FakeClassed` sequence
# depends on `TranscriptPane._compute_new_top`'s actual arithmetic -- a wrong
# 302-entry fold or either odd-cut arm would still produce a `_FakePane`
# shaped exactly like a correct one, if the fixture is built by hand rather
# than by driving the real pane. The tests below drive a real
# `TranscriptPane` through its own `apply()`/`_condense()` over the three
# exact shapes the plan pins (U4's aggregate-ceiling, odd-cut, and
# partial-retention regressions), and check `fallback_banner_accounting`
# against the DOM that arithmetic actually produced.
#
# The geometry (140 columns, not this module's own `GATE_SIZE`) is chosen
# wide enough that `FALLBACK_BANNER_TEMPLATE`'s own fixed ~126-character
# message does not itself wrap past one row -- that is a separate claim,
# pinned above and by `test_fallback_banner_accounting_fails_when_the_
# banner_wraps_past_one_row`, and conflating it with these fold-arithmetic
# fixtures would make a fold-count regression indistinguishable from a
# banner-width regression. Correspondingly, `_BIG_LINE` is long enough to
# still trip `trips_fallback_trigger`'s wrapped-row condition at this wider
# content width.

_FOLD_SIZE = (140, 40)
_BIG_LINE = "x" * 80_000


def _one_line_fallback_entries(n: int, *, start_id: int = 1) -> tuple[TranscriptEntryRecord, ...]:
    """``n`` one-line fallen-back entries (KTD1(a)): a single line long
    enough to trip the wrapped-row trigger at :data:`_FOLD_SIZE`'s content
    width, each contributing exactly one projected line -- the exact shape
    the plan's aggregate-ceiling and odd-cut regressions are stated over.
    """
    return tuple(
        TranscriptEntryRecord(
            entry_id=start_id + i,
            kind="assistant",
            raw_body=_BIG_LINE,
            committed=True,
            line_span=(i, 1),
        )
        for i in range(n)
    )


@asynccontextmanager
async def _apply_fold_fixture(
    records: tuple[TranscriptEntryRecord, ...], lines: tuple[str, ...]
) -> AsyncIterator[TranscriptPane]:
    """Yields the pane with ``app.run_test()``'s context still open --
    returning it *after* that context exits would hand back a pane whose
    widgets ``run_test``'s own teardown already unmounted.
    """

    class _PaneHarness(App[None]):
        def compose(self) -> ComposeResult:
            yield TranscriptPane(id="t")

    app = _PaneHarness()
    async with app.run_test(size=_FOLD_SIZE):
        pane = app.query_one(TranscriptPane)
        view = TranscriptView(lines=lines, entry_count=len(records))
        entries = EntryScopedView(
            entries=records,
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=0),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="", generation=0),
        )
        await pane.apply(view, entries)
        yield pane


@pytest.mark.asyncio
async def test_the_real_fold_holds_302_fallen_back_entries_at_the_aggregate_ceiling() -> None:
    """The plan's exact aggregate-ceiling shape: 302 one-line fallen-back
    entries, each accounted at two rows (content + banner). A lines-only
    fold trigger (ignoring the banner charge) would mount all 301 non-newest
    entries -- 602 widgets -- breaching the 600 descendant ceiling; the real
    fold must instead hold the folded window at <= 500 accounted rows.
    """
    records = _one_line_fallback_entries(302)
    async with _apply_fold_fixture(records, tuple(_BIG_LINE for _ in range(302))) as pane:
        assert len(pane._entry_order) == 250, "250 entries fit two accounted rows each into 500"
        assert pane.condensed_count == 52, "the other 52 fold away, oldest first"
        # 250 entries x (1 content widget + 1 banner widget) + 1 condensation banner.
        assert pane.mounted_count == 501
        assert pane.descendant_count <= DESCENDANT_WIDGET_CEILING

        ok, reason = fallback_banner_accounting(pane)
        assert ok, reason
        block_ok, block_reason = block_documents_are_owned(pane)
        assert block_ok, block_reason


@pytest.mark.asyncio
async def test_the_real_fold_rounds_forward_on_the_odd_cut_regression() -> None:
    """The plan's exact odd-cut shape: 250 one-line fallen-back entries plus
    one ordinary line (501 accounted rows total). `desired_top = 1` lands
    inside the oldest (fallen-back) entry's own two-row span -- the fold
    must round forward and take the whole entry, banner included, never
    leaving a bannerless clipped row or an orphan banner.
    """
    records = _one_line_fallback_entries(250) + (
        TranscriptEntryRecord(
            entry_id=251, kind="user", raw_body="ordinary", committed=True, line_span=(250, 1)
        ),
    )
    lines = tuple(_BIG_LINE for _ in range(250)) + ("› ordinary",)
    async with _apply_fold_fixture(records, lines) as pane:
        assert pane.condensed_count == 1, "the fold rounds forward over the whole oldest entry"
        assert 1 not in pane._entries, "the fallen-back entry -- banner included -- is gone"
        assert list(pane._entry_order)[0] == 2, "eviction never leaves a bannerless remainder"

        ok, reason = fallback_banner_accounting(pane)
        assert ok, reason
        block_ok, block_reason = block_documents_are_owned(pane)
        assert block_ok, block_reason


@pytest.mark.asyncio
async def test_the_real_fold_partially_retains_the_two_line_entry_and_keeps_its_one_banner() -> (
    None
):
    """The plan's exact partial-retention shape: a two-line fallen-back
    entry plus 498 ordinary lines (501 accounted rows total).
    `desired_top = 1` folds exactly one content row of the oldest entry,
    which then retains one content row **plus exactly one banner**
    (painted rows = retained projected lines + 1 = 2) -- the arm an
    always-round-forward implementation (which would pass the odd-cut test
    above) fails, which is why the plan pins both.
    """
    records = (
        TranscriptEntryRecord(
            entry_id=1,
            kind="assistant",
            raw_body=f"{_BIG_LINE}\n{_BIG_LINE}",
            committed=True,
            line_span=(0, 2),
        ),
        *(
            TranscriptEntryRecord(
                entry_id=2 + i,
                kind="user",
                raw_body=f"line{i}",
                committed=True,
                line_span=(2 + i, 1),
            )
            for i in range(498)
        ),
    )
    lines = (_BIG_LINE, _BIG_LINE) + tuple(f"› line{i}" for i in range(498))
    async with _apply_fold_fixture(records, lines) as pane:
        assert pane.condensed_count == 1
        retained = pane._entries[1]
        assert retained.line_count == 1, "one content row folded away, one retained"
        assert retained.banner is not None, "the retained row still carries exactly one banner"

        ok, reason = fallback_banner_accounting(pane)
        assert ok, reason
        block_ok, block_reason = block_documents_are_owned(pane)
        assert block_ok, block_reason


@pytest.mark.asyncio
async def test_removing_a_banner_from_a_real_fold_makes_the_accounting_fail() -> None:
    """The mutation twin of the three tests above: the real 302-entry fold
    is proven correct, then one of its own banners -- produced by the real
    fold arithmetic, not a hand-built fake -- is torn off, and the proof
    must notice. This is exactly what a class-only `_FakePane` fixture
    cannot exercise: there is no real fold to corrupt.

    The *newest* mounted entry's banner is the one removed, deliberately:
    every other fallen-back entry sits immediately in front of the next
    one's own banner, so tearing off an interior banner only merges two
    adjacent content runs into one still-followed-by-a-real-banner run --
    correctly not a violation, since RA3 requires a banner after every
    maximal run, not one banner per original entry. Only the trailing
    banner has no following banner to be absorbed into.
    """
    records = _one_line_fallback_entries(302)
    async with _apply_fold_fixture(records, tuple(_BIG_LINE for _ in range(302))) as pane:
        ok, reason = fallback_banner_accounting(pane)
        assert ok, reason

        newest_mounted = pane._entry_order[-1]
        banner = pane._entries[newest_mounted].banner
        assert banner is not None
        await banner.remove()

        ok, reason = fallback_banner_accounting(pane)
        assert ok is False
        assert "no banner" in reason


# ── progressiveness, including R18's two-tail overlap ─────────────────────


@pytest.mark.asyncio
async def test_mounted_window_ownership_holds_mid_stream_for_two_growing_tails_at_once() -> None:
    """R18: within one turn, reasoning precedes the reply it explains, and
    the domain holds both buffers at once — neither may steal the other's
    progressive rendering. This proves the mounted-window half of the
    ownership proof (unlike the line-window half) is not a race mid-stream:
    both tail documents are checked while each is still growing, at three
    successive intermediate snapshots that overlap in time, not only once
    both have settled.
    """
    from talaria.domain.projection import EntryScopedView, ProvisionalTail
    from talaria.ui.transcript import TranscriptPane

    class _PaneHarness(App[None]):
        def compose(self) -> ComposeResult:
            yield TranscriptPane(id="t")

    # Three snapshots of the same turn, each tail mid-construct at a
    # different point -- a heading still being typed, then a completed
    # heading with a paragraph starting, then both further along. Neither
    # snapshot is a settled document, and the two tails overlap at every one
    # of them.
    snapshots = [
        ("# reasoning heading", "assistant text growin"),
        ("# reasoning heading\n\nmore reasoning", "assistant text growing more"),
        ("# reasoning heading\n\nmore reasoning still", "assistant text growing more still"),
    ]

    app = _PaneHarness()
    async with app.run_test() as pilot:
        pane = app.query_one(TranscriptPane)
        for generation, (reasoning_text, assistant_text) in enumerate(snapshots):
            entries = EntryScopedView(
                entries=(),
                assistant_tail=ProvisionalTail(
                    kind="assistant", raw_text=assistant_text, generation=generation
                ),
                reasoning_tail=ProvisionalTail(
                    kind="reasoning", raw_text=reasoning_text, generation=generation
                ),
            )
            view = TranscriptView(
                lines=tuple(reasoning_text.split("\n")) + tuple(assistant_text.split("\n")),
                entry_count=0,
            )
            await pane.apply(view, entries)
            await pilot.pause()

            documents = list(pane.query(EntryMarkdown))
            assert len(documents) == 2, (
                f"snapshot {generation}: both tails must be mounted as block documents "
                f"simultaneously, got {len(documents)}"
            )
            ok, reason = block_documents_are_owned(pane)
            assert ok, f"snapshot {generation}: {reason}"


# ── R11a guard: content_is_complete's callers still compose correctly ─────


@pytest.mark.asyncio
async def test_ownership_report_composes_all_three_halves_and_names_which_failed() -> None:
    """`interface_shows_everything` collapses this to a bool for the gate's
    existing call sites; this pins that the composed, named-reason surface
    (`ownership_report`) is what a mutation test or a future gate-results
    document would actually read to say *which* half failed.
    """
    from talaria.replay.controls import ReplayControls
    from talaria.replay.source import ReplaySource
    from talaria.ui.app import TalariaApp

    controls = ReplayControls(paused=True)
    source = ReplaySource((), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test() as pilot:
        await pilot.pause()
        ok, reasons = ownership_report(app, settled=True)
        # An empty replay: nothing mounted, nothing condensed, nothing owed.
        assert ok, reasons
        assert reasons == ()
        await app.shutdown_sources()


# ── the two-sided proof: an expected document can also be absent (CR6) ────


@pytest.mark.asyncio
async def test_a_reasoning_only_delta_withheld_from_the_pane_is_not_accepted_vacuously() -> None:
    """CR6's plausible finding at gate.py:524: :func:`block_documents_are_owned`
    only ever walks what is *already* mounted, so a delta that never reaches
    :meth:`TranscriptPane.apply` at all leaves nothing missing *from* that
    walk to notice — the absent document passes vacuously. Reproduced here
    by writing directly to ``app.state`` (a reasoning delta lands in the
    domain) without ever calling :meth:`TalariaApp.render_snapshot` again —
    the only call site that reconciles the pane — so the pane stays exactly
    as it was: nothing mounted, and nothing wrong with what is mounted.
    :func:`expected_block_documents_are_mounted` is the fix: it derives what
    should be mounted straight from the domain's own current
    ``entry_scoped_view`` rather than from what already made it onto the
    pane, so it is the half of the composed proof that actually notices.
    """
    from talaria.domain.state import SessionState
    from talaria.replay.controls import ReplayControls
    from talaria.replay.source import ReplaySource
    from talaria.ui.app import TalariaApp

    controls = ReplayControls(paused=True)
    source = ReplaySource((), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test() as pilot:
        await pilot.pause()

        app.state = SessionState(
            reasoning_text="# a reasoning heading\n\nmore reasoning",
            reasoning_stream_generation=1,
        )

        # The old, one-sided check: nothing is mounted, and nothing that IS
        # mounted is wrong, so it passes -- the vacuous accept CR6 named.
        ok, reason = block_documents_are_owned(app.transcript)
        assert ok, reason

        # The new, two-sided check: the domain's own current view says the
        # reasoning tail should be a block document by now, and it is not.
        ok, reason = expected_block_documents_are_mounted(app)
        assert ok is False
        assert "reasoning" in reason

        # The composed proof inherits the fix.
        settled_ok, reasons = ownership_report(app, settled=True)
        assert settled_ok is False
        assert any("reasoning" in r for r in reasons)

        await app.shutdown_sources()
