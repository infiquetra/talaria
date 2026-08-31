"""U4's hybrid pane: block documents, line widgets, two live tails, and
condensation over mixed units (KTD1, KTD2, KTD3; R1, R2, R5, R16, R17, R18).

Every test here drives :meth:`TranscriptPane.apply` directly with hand-built
:class:`~talaria.domain.projection.TranscriptView` and
:class:`~talaria.domain.projection.EntryScopedView` snapshots, rather than
through a live app — the pane's contract is exactly "given these two
projections, mount and fold correctly", and testing at that seam is what
lets a fixture describe an exact accounted-row shape (the aggregate-ceiling,
odd-cut, and partial-retention regressions each need one) without assembling
a gateway transcript that happens to produce it.

**One caveat this suite makes visible rather than hiding.** ``view.lines`` —
what :func:`~talaria.domain.projection.transcript_view` (a U2 function, not
touched here) produces — never includes the in-flight *reasoning* buffer;
only the assistant tail is folded into that flattened line buffer, a gap
this unit's grounding read found and could not close without editing
``talaria/domain/projection.py``, outside U4's file list. The reconstruction
invariant ``pane.rendered_lines == view.lines[pane.condensed_count:]`` is
therefore proven here for every scenario where the two projections actually
agree (committed content, and assistant-tail streaming) and *not* claimed for
a concurrently streaming reasoning tail — that gap is a real, open follow-up,
not a rendering defect in this pane.
"""

from __future__ import annotations

import gc
import re
import weakref
from collections.abc import Sequence
from typing import cast

import pytest
from textual.app import App, ComposeResult
from textual.content import Content
from textual.widget import Widget
from textual.widgets._markdown import (
    MarkdownBlockQuote,
    MarkdownBulletList,
    MarkdownFence,
    MarkdownH1,
    MarkdownOrderedList,
    MarkdownParagraph,
    MarkdownTable,
    MarkdownTableCellContents,
)
from textual.widgets.markdown import MarkdownBlock

from talaria.domain.models import TranscriptKind
from talaria.domain.projection import (
    # The weld prefixes come from the DOMAIN projection, not from
    # transcript.py: _view_for computes the expected view, and expecting the
    # pane's own prefix table would let a pane-side drift move both sides of
    # every assertion together (CR4 finding 4). The pane must match the
    # projection; the projection is what the fixture mirrors.
    _ENTRY_PREFIX,
    EntryScopedView,
    ProvisionalTail,
    TranscriptEntryRecord,
    TranscriptView,
)
from talaria.ui.blocks import EntryMarkdown
from talaria.ui.theme import BUILTIN_THEME_REGISTRY
from talaria.ui.transcript import (
    DEFAULT_MOUNT_CAP,
    DESCENDANT_ESTIMATE_TRIGGER,
    TranscriptLine,
    TranscriptPane,
    descendant_estimate,
    is_zero_block,
    trips_fallback_trigger,
    wrapped_row_estimate,
)


class _Harness(App[None]):
    def __init__(self, mount_cap: int = DEFAULT_MOUNT_CAP) -> None:
        super().__init__()
        BUILTIN_THEME_REGISTRY.register(self)
        self.theme = "refined-default"
        self._mount_cap = mount_cap

    def compose(self) -> ComposeResult:
        yield TranscriptPane(mount_cap=self._mount_cap, id="t")


def _empty_tail(kind: str) -> ProvisionalTail:
    return ProvisionalTail(kind=kind, raw_text="", generation=0)  # type: ignore[arg-type]


def _view_for(
    entries: Sequence[TranscriptEntryRecord], assistant_tail: ProvisionalTail | None = None
) -> TranscriptView:
    """Mirrors ``transcript_view()``'s actual shape: committed entries plus
    (only) the assistant tail — see the module docstring's caveat.
    """
    lines: list[str] = []
    kinds: list[TranscriptKind] = []
    for record in entries:
        welded = f"{_ENTRY_PREFIX.get(record.kind, '')}{record.raw_body}"
        body_lines = welded.split("\n") if welded else [""]
        lines.extend(body_lines)
        kinds.extend([record.kind] * len(body_lines))
    committed = len(lines)
    tail = assistant_tail or _empty_tail("assistant")
    if tail.raw_text:
        # splitlines, matching the real projection's tail branch (a trailing
        # newline adds no row) — split("\n") here reproduced the production
        # off-by-one instead of catching it (CR1 finding 4).
        tail_lines = tail.raw_text.splitlines() or [""]
        lines.extend(tail_lines)
        kinds.extend(["assistant"] * len(tail_lines))
    return TranscriptView(
        lines=tuple(lines), entry_count=len(entries), committed_lines=committed, kinds=tuple(kinds)
    )


async def _apply(
    pane: TranscriptPane,
    entries: Sequence[TranscriptEntryRecord],
    *,
    assistant_tail: ProvisionalTail | None = None,
    reasoning_tail: ProvisionalTail | None = None,
) -> TranscriptView:
    view = _view_for(entries, assistant_tail)
    esv = EntryScopedView(
        entries=tuple(entries),
        assistant_tail=assistant_tail or _empty_tail("assistant"),
        reasoning_tail=reasoning_tail or _empty_tail("reasoning"),
    )
    await pane.apply(view, esv)
    return view


def _fallback_entries(
    n: int, *, start_id: int = 1, body_len: int = 100_000
) -> tuple[TranscriptEntryRecord, ...]:
    out = []
    for i in range(n):
        out.append(
            TranscriptEntryRecord(
                entry_id=start_id + i,
                kind="assistant",
                raw_body="x" * body_len,
                committed=True,
                line_span=(i, 1),
            )
        )
    return tuple(out)


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("kind", "label"),
    [("assistant", "A Talaria"), ("reasoning", ". Reasoning")],
)
async def test_block_entries_add_the_fixed_label_without_changing_raw_markdown(
    kind: TranscriptKind,
    label: str,
) -> None:
    raw = "# heading\n\nbody with **emphasis**"
    entry = TranscriptEntryRecord(
        entry_id=1,
        kind=kind,
        raw_body=raw,
        committed=True,
        line_span=(0, 3),
    )
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one(TranscriptPane)
        await _apply(pane, (entry,))
        await pilot.pause()

        unit = pane._entries[1]
        assert unit.block is not None
        first = next(
            block
            for block in unit.block.query(MarkdownBlock)
            if cast(Content, block.content).plain
        )
        content = cast(Content, first.content)
        assert content.plain.startswith(f"{label}  ")
        assert content.get_style_at_offset(0).bold is True
        assert unit.applied_text == raw
        assert unit.block.source == raw


# ── construct-aware estimate calibration (KTD1(a)) ──────────────────────────


def _table(rows: int, cols: int) -> str:
    header = "|" + "|".join("h" for _ in range(cols)) + "|"
    delim = "|" + "|".join("---" for _ in range(cols)) + "|"
    body = "\n".join("|" + "|".join("x" for _ in range(cols)) + "|" for _ in range(rows))
    return f"{header}\n{delim}\n{body}\n"


def test_601_column_table_descendant_estimate_trips_the_trigger() -> None:
    """Probed live against Textual 8.2.8: a 3-line, 601-column table mounts
    1,204 descendants. The estimate must reproduce that exactly (1,202 cells
    plus 2 table-container overhead), not merely exceed the trigger by luck.
    """
    text = _table(1, 601)
    assert descendant_estimate(text) == 1_204
    assert descendant_estimate(text) > DESCENDANT_ESTIMATE_TRIGGER


def test_exact_boundary_599_column_table_plus_paragraph() -> None:
    """The pinned exact-boundary regression: top-level blocks plus cells
    estimate to exactly 1,201 (1,200 table + 1 paragraph), matching the
    installed widget's own probed 1,201 descendants, and 1,201 > 1,200 fires
    the trigger — a naive top-level-block count (2) would not.
    """
    text = _table(1, 599) + "\n" + ("word " * 20) + "\n"
    estimate = descendant_estimate(text)
    assert estimate == 1_201
    assert estimate > DESCENDANT_ESTIMATE_TRIGGER


def test_10000_line_fence_trips_on_wrapped_rows_not_descendants() -> None:
    """A 10,000-line open fence mounts as one block (two descendants,
    probed) yet paints 10,000+ rows — the descendant condition alone would
    never catch it, which is exactly why the trigger is two conditions,
    either sufficient.
    """
    fence = "```\n" + "\n".join(f"line{i}" for i in range(10_000)) + "\n```\n"
    assert descendant_estimate(fence) <= DESCENDANT_ESTIMATE_TRIGGER
    assert wrapped_row_estimate(fence, 80) > DEFAULT_MOUNT_CAP
    assert trips_fallback_trigger(fence, content_width=80)


def test_double_width_line_estimated_in_display_cells_not_characters() -> None:
    """A double-width-character line must not be undercounted by a character
    count — the module's own docstring names a 37,000-character CJK line
    painting 949 rows against a 475-row character estimate; this pins the
    *direction* of the fix (display cells, not ``len()``) against a smaller
    reproducible fixture.
    """
    cjk = "中" * 25_000
    cell_estimate = wrapped_row_estimate(cjk, 80)
    naive_char_estimate = -(-len(cjk) // 80)
    assert cell_estimate > naive_char_estimate
    assert trips_fallback_trigger(cjk, content_width=80)


def test_ordinary_short_entry_does_not_trip_the_trigger() -> None:
    assert not trips_fallback_trigger("# heading\n\nshort paragraph\n", content_width=80)


def test_zero_block_sources_are_recognized() -> None:
    assert is_zero_block("")
    assert is_zero_block("   \n  \n")
    assert not is_zero_block("# heading\n")


# ── mounting: block vs. line, per KTD2's rules ──────────────────────────────


@pytest.mark.asyncio
async def test_committed_assistant_entry_mounts_one_markdown_document() -> None:
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entries = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body="# Title\n\nBody text.",
                committed=True,
                line_span=(0, 3),
            ),
        )
        view = await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.kind == "block"
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_zero_block_entry_line_renders_preserving_blank_rows() -> None:
    """An empty/whitespace-only assistant entry never mounts a height-zero
    document (probed) — it line-renders, and every blank row it had stays a
    visible line widget, no banner (this is not a fallback, just empty
    content).
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body="   \n\n  ", committed=True, line_span=(0, 3)
            ),
        )
        view = await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.kind == "line"
        assert unit.banner is None
        assert len(unit.lines) == 3
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_non_markdown_kind_stays_line_rendered_with_its_weld() -> None:
    """``user``/``tool`` never enter MARKDOWN_KINDS; ``rendered_lines`` still
    has to match ``TranscriptView.lines``, weld included (a real bug found
    while building this unit: entry_scoped_view's raw_body is deliberately
    unwelded, and the pane has to re-apply the weld itself for line-rendered
    surfaces or this equality silently breaks for every non-agent kind).
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="user", raw_body="what changed?", committed=True, line_span=(0, 1)
            ),
        )
        view = await _apply(pane, entries)
        assert view.lines[0] == "› what changed?"
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


# ── streaming: progressive fences, generation-based replace, two tails ─────


@pytest.mark.asyncio
async def test_fence_streams_progressively_at_boundaries() -> None:
    """A fence streamed opener, then body, then closer renders structure at
    each surviving boundary (AE1) — the tail is a real block document from
    the first delta, growing by ``append``, never re-parsed from scratch.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        gen = 1
        opener = ProvisionalTail(kind="assistant", raw_text="```python\n", generation=gen)
        await _apply(pane, (), assistant_tail=opener)
        tail = pane._tails["assistant"]
        assert tail is not None and tail.kind == "block"
        opener_widget = tail.block

        body = ProvisionalTail(kind="assistant", raw_text="```python\nx = 1\n", generation=gen)
        await _apply(pane, (), assistant_tail=body)
        tail = pane._tails["assistant"]
        assert tail is not None
        assert tail.block is opener_widget, "same generation appends in place, no rebuild"
        assert tail.applied_text == body.raw_text

        closer = ProvisionalTail(
            kind="assistant", raw_text="```python\nx = 1\n```\n", generation=gen
        )
        await _apply(pane, (), assistant_tail=closer)
        tail = pane._tails["assistant"]
        assert tail is not None
        assert tail.block is opener_widget
        document = tail.block
        assert document is not None
        fences = list(document.query(MarkdownFence))
        assert len(fences) == 1, "closed fence is one bounded region"


@pytest.mark.asyncio
async def test_table_first_mounted_mid_table_survives_every_append_and_clean_commit() -> None:
    """The defect this pass was asked to fix
    (docs/analysis/2026-08-09-block-markdown-gate-results.md, "A real defect
    this work found"): Textual 8.2.8's `Markdown.update` — the same code
    path a widget's initial mount takes via `_on_mount` — seeds the private
    append checkpoint `_last_parsed_line` from "total lines minus one",
    correct only when the last construct is exactly one line long at that
    moment. A table can only ever become block-eligible with a header, a
    separator, and at least one row already present, so the tail's *first*
    mounted text is never just the table's opening line — every table tail
    takes this path. Uncorrected, the next `append` reparses a window
    missing the header, which parses as a bare paragraph.

    Three deltas (a real gateway streams one row at a time), then a clean
    `message.complete` commit reporting exactly the streamed text — the
    exact shape that used to leave a corrupted `MarkdownParagraph` in the
    *committed*, settled transcript forever, because the commit handoff
    used to skip its corrective `update()` whenever the committed text
    already equalled what the tail had applied (always true for a clean
    completion). Verified to fail against the unfixed code: reverting
    `EntryMarkdown._correct_last_parsed_line` (blocks.py) fails this at the
    second delta with a lone `MarkdownParagraph`; reverting the
    unconditional commit-handoff `update()` (transcript.py) alone, with the
    live-tail fix still in place, does not reproduce it (layer one already
    keeps the tail correct here) — the two-layer coverage is why the
    dedicated corruption test below forces layer one's failure mode by hand
    to prove layer two independently.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        gen = 1

        first = ProvisionalTail(
            kind="assistant", raw_text="| col |\n| --- |\n| r1 |", generation=gen
        )
        await _apply(pane, (), assistant_tail=first)
        await pilot.pause()
        tail = pane._tails["assistant"]
        assert tail is not None and tail.kind == "block"
        widget = tail.block
        assert widget is not None
        assert list(widget.query(MarkdownTable)), "the first mounted delta must be a table"

        second = ProvisionalTail(
            kind="assistant", raw_text="| col |\n| --- |\n| r1 |\n| r2 |", generation=gen
        )
        await _apply(pane, (), assistant_tail=second)
        await pilot.pause()
        tail = pane._tails["assistant"]
        assert tail is not None
        assert tail.block is widget, "same generation appends in place, no rebuild"
        assert list(widget.query(MarkdownTable)), "delta two must still be a table"

        third = ProvisionalTail(
            kind="assistant",
            raw_text="| col |\n| --- |\n| r1 |\n| r2 |\n| r3 |",
            generation=gen,
        )
        await _apply(pane, (), assistant_tail=third)
        await pilot.pause()
        tail = pane._tails["assistant"]
        assert tail is not None
        assert tail.block is widget
        assert list(widget.query(MarkdownTable)), "delta three must still be a table"

        entries = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body=third.raw_text,
                committed=True,
                line_span=(0, 5),
            ),
        )
        await _apply(
            pane,
            entries,
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=gen + 1),
        )
        await pilot.pause()
        committed_unit = pane._entries[1]
        assert committed_unit.block is widget, "commit reuses the tail's widget"
        assert list(widget.query(MarkdownTable)), (
            "the committed, settled document must still be a table after "
            "clean completion — this is the defect's permanent half"
        )


@pytest.mark.asyncio
async def test_fence_first_mounted_mid_fence_survives_every_append_and_clean_commit() -> None:
    """The open-construct variant of the same defect: a fence tail whose
    first mounted text already has more than the opening delimiter line —
    ordinary once a coalescing render tick catches a fence after its first
    few lines have already streamed — is seeded the same wrong way,
    pointing the append checkpoint at the fence's last content line rather
    than its own opening ```` ``` ```` line.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        gen = 1

        first = ProvisionalTail(
            kind="assistant", raw_text="```python\nx = 1\ny = 2", generation=gen
        )
        await _apply(pane, (), assistant_tail=first)
        await pilot.pause()
        tail = pane._tails["assistant"]
        assert tail is not None and tail.kind == "block"
        widget = tail.block
        assert widget is not None
        assert list(widget.query(MarkdownFence)), "the first mounted delta must be a fence"

        second = ProvisionalTail(
            kind="assistant", raw_text="```python\nx = 1\ny = 2\nz = 3", generation=gen
        )
        await _apply(pane, (), assistant_tail=second)
        await pilot.pause()
        tail = pane._tails["assistant"]
        assert tail is not None
        assert tail.block is widget, "same generation appends in place, no rebuild"
        assert list(widget.query(MarkdownFence)), "delta two must still be a fence"

        closed_text = "```python\nx = 1\ny = 2\nz = 3\n```\ntrailing paragraph\n"
        third = ProvisionalTail(kind="assistant", raw_text=closed_text, generation=gen)
        await _apply(pane, (), assistant_tail=third)
        await pilot.pause()
        tail = pane._tails["assistant"]
        assert tail is not None
        assert tail.block is widget
        fences = list(widget.query(MarkdownFence))
        assert len(fences) == 1, "closing the fence must still be one bounded region"
        assert fences[0].code == "x = 1\ny = 2\nz = 3"

        entries = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body=closed_text,
                committed=True,
                line_span=(0, 7),
            ),
        )
        await _apply(
            pane,
            entries,
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=gen + 1),
        )
        await pilot.pause()
        committed_unit = pane._entries[1]
        assert committed_unit.block is widget
        assert list(widget.query(MarkdownFence)), (
            "the committed, settled document must still contain the fence "
            "after clean completion"
        )


@pytest.mark.asyncio
async def test_commit_handoff_rebuilds_a_structurally_corrupted_tail_unconditionally() -> None:
    """Layer two's own proof, independent of blocks.py's live-tail fix: even
    a tail widget that somehow carries structural corruption at commit time
    must still hand off a structurally correct committed document, because
    the corrective `update()` on the tail-to-entry handoff
    (`TranscriptPane._prepare_committed_entry`) now runs unconditionally —
    not only when the committed text differs from what the tail last had
    applied.

    The corruption here is forced by hand rather than reproduced through
    real streaming (the test above already does that for layer one): set
    the private append checkpoint to the exact wrong value Textual 8.2.8's
    own uncorrected seeding heuristic would have produced, then append
    directly against the widget — bypassing `EntryMarkdown.update`'s
    correction entirely, since only `update()` corrects the checkpoint, not
    `append()`. This isolates layer two: even with layer one's fix
    unable to help (because nothing here calls `update()` before the
    append), the commit handoff must still repair it. Verified to fail
    against the unfixed code: restoring the old `if record.raw_body !=
    tail.applied_text:` guard makes this a no-op, since `record.raw_body`
    is set equal to `tail.applied_text` below — exactly the clean-
    completion shape the guard used to treat as "nothing to do".
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        table_text = "| col |\n| --- |\n| r1 |"
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=table_text, generation=1),
        )
        await pilot.pause()
        tail = pane._tails["assistant"]
        assert tail is not None and tail.kind == "block"
        widget = tail.block
        assert widget is not None
        assert list(widget.query(MarkdownTable)), "starts correct (layer one already fixed this)"

        # Force the pre-fix corruption directly, bypassing update()'s
        # correction: point the append checkpoint at the table's last row
        # instead of its own start line, then append — reparsing a window
        # with no header/separator in view, exactly Textual's own
        # uncorrected `total lines - 1` heuristic would have done.
        widget._last_parsed_line = len(table_text.splitlines()) - 1
        await widget.append("\n| r2 |")
        await pilot.pause()
        tail.applied_text = table_text + "\n| r2 |"
        corrupted_types = {type(w).__name__ for w in widget.walk_children()}
        assert "MarkdownTable" not in corrupted_types, "the forced corruption itself"

        entries = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body=tail.applied_text,
                committed=True,
                line_span=(0, 4),
            ),
        )
        await _apply(
            pane,
            entries,
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=2),
        )
        await pilot.pause()

        committed_unit = pane._entries[1]
        assert committed_unit.block is widget, "commit still hands off the same widget object"
        committed_types = {type(w).__name__ for w in widget.walk_children()}
        assert "MarkdownTable" in committed_types, (
            "the commit handoff must rebuild a structurally correct "
            "document even from a corrupted tail — the corrective update() "
            "now runs unconditionally rather than being skipped as a "
            "text-equality no-op"
        )


@pytest.mark.asyncio
async def test_interim_replacement_renders_exactly_once_via_generation() -> None:
    """A changed generation calls ``update()`` with the authoritative text —
    never a prefix guess appended on top of stale content (AE7).
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="draft one", generation=1),
        )
        # message.interim replaces wholesale with unrelated text and a bumped
        # generation -- appending would silently corrupt this into "draft
        # oneinterim replacement, authoritative".
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(
                kind="assistant", raw_text="interim replacement, authoritative", generation=2
            ),
        )
        tail = pane._tails["assistant"]
        assert tail is not None
        assert tail.applied_text == "interim replacement, authoritative"
        document = tail.block
        assert document is not None
        painted = "\n".join(str(p.render()) for p in document.query(MarkdownParagraph))
        assert "draft one" not in painted
        assert "interim replacement, authoritative" in painted


@pytest.mark.asyncio
async def test_block_tail_collapsing_to_zero_blocks_demotes_to_line_rendering() -> None:
    """A block-rendered tail whose *next* generation collapses it to zero
    blocks (a bare newline) must demote to line rendering, not stay mounted
    as an invisible height-zero :class:`~talaria.ui.blocks.EntryMarkdown`
    forever.

    ``_reconcile_tail``'s block-kind branch used to re-check only
    ``trips_fallback_trigger`` after writing the grown/replaced text --
    exactly what ``is_zero_block`` on the line-kind branch already guards
    against, but missing here. ``is_zero_block("\\n")`` is ``True`` while
    ``trips_fallback_trigger("\\n", ...)`` is ``False`` (nowhere near the
    size trigger), so the old code never rebuilt: the tail stayed a
    ``block``-kind unit pointing at a document Textual mounts at height 0
    for zero parsed blocks (KTD2's own zero-block rule, proven for
    committed entries by ``test_zero_block_entry_line_renders_preserving_
    blank_rows`` above but not, until this fix, for a tail that *became*
    zero-block after already being a block document).
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            reasoning_tail=ProvisionalTail(
                kind="reasoning",
                raw_text="# heading\n\nsome reasoning prose here\n",
                generation=1,
            ),
        )
        await pilot.pause()
        grown = pane._tails["reasoning"]
        assert grown is not None and grown.kind == "block"

        # A new generation replaces the block wholesale with a bare newline
        # -- zero parsed blocks, same shape as message.interim discarding a
        # stream's content outright.
        await _apply(
            pane,
            (),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="\n", generation=2),
        )
        await pilot.pause()
        collapsed = pane._tails["reasoning"]
        assert collapsed is not None
        assert collapsed.kind == "line", (
            "still block-rendered -- the zero-block collapse was missed"
        )
        assert collapsed.lines, "the blank row vanished instead of becoming a visible line widget"
        assert all(isinstance(widget, TranscriptLine) for widget in collapsed.lines)
        for widget in collapsed.lines:
            assert widget.outer_size.height >= 1

        # No stray height-zero EntryMarkdown left mounted anywhere in the pane.
        assert not list(pane.query(EntryMarkdown))


@pytest.mark.asyncio
async def test_both_tails_stream_independently_in_the_same_turn() -> None:
    """R18: reasoning and assistant can stream in the same turn; neither may
    steal the other's progressive rendering.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="answering", generation=1),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="thinking", generation=1),
        )
        assistant_tail = pane._tails["assistant"]
        reasoning_tail = pane._tails["reasoning"]
        assert assistant_tail is not None
        assert reasoning_tail is not None
        a_widget = assistant_tail.block
        r_widget = reasoning_tail.block
        assert a_widget is not r_widget

        # Advance only the reasoning tail -- the assistant tail's widget and
        # text must not move.
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="answering", generation=1),
            reasoning_tail=ProvisionalTail(
                kind="reasoning", raw_text="thinking more", generation=1
            ),
        )
        assistant_tail = pane._tails["assistant"]
        reasoning_tail = pane._tails["reasoning"]
        assert assistant_tail is not None
        assert reasoning_tail is not None
        assert assistant_tail.block is a_widget
        assert assistant_tail.applied_text == "answering"
        assert reasoning_tail.applied_text == "thinking more"


@pytest.mark.asyncio
async def test_commit_hands_tail_source_to_entry_document_without_rebuild() -> None:
    """KTD2: on commit, the tail's final source becomes its entry's document
    — the same widget object, keyed by entry id, no pane rebuild.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="final reply", generation=1),
        )
        assistant_tail = pane._tails["assistant"]
        assert assistant_tail is not None
        tail_widget = assistant_tail.block

        entries = (
            TranscriptEntryRecord(
                entry_id=7,
                kind="assistant",
                raw_body="final reply",
                committed=True,
                line_span=(0, 1),
            ),
        )
        await _apply(
            pane,
            entries,
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="", generation=2),
        )
        assert pane._tails["assistant"] is None
        committed_unit = pane._entries[7]
        assert committed_unit.block is tail_widget, "commit reuses the tail's widget, no rebuild"


@pytest.mark.asyncio
async def test_stale_tail_write_after_removal_updates_nothing_and_raises_nothing() -> None:
    """R16 clause 3: a write against a widget that is no longer mounted is a
    silent no-op.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="hello", generation=1),
        )
        assistant_tail = pane._tails["assistant"]
        assert assistant_tail is not None
        widget = assistant_tail.block
        assert widget is not None
        await widget.remove()
        # No exception -- the guard checks is_mounted before writing.
        await pane._safe_write(widget, "append", " more")
        await pane._safe_write(widget, "update", "replacement")


# ── fallback rendering (KTD1(a), RA3) ───────────────────────────────────────


@pytest.mark.asyncio
async def test_oversized_entry_falls_back_to_nonwrapping_lines_plus_one_banner() -> None:
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entries = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body="x" * 100_000,
                committed=True,
                line_span=(0, 1),
            ),
        )
        view = await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.kind == "line" and unit.is_fallback
        assert len(unit.lines) == 1
        assert unit.banner is not None
        assert unit.accounted_rows == 2, "painted rows == projected lines + one banner row"
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
@pytest.mark.parametrize("pane_width", [40, 80, 100])
async def test_fallback_banner_paints_exactly_one_row(pane_width: int) -> None:
    """The RA3 banner must paint exactly one row at every width, matching
    the plan's exact-row formula, the replay gate's
    ``fallback_banner_accounting`` check, and this module's own
    ``accounted_rows`` charge (one banner row per fallen-back entry, see
    the test above). An unconstrained ``Static`` wraps
    :data:`~talaria.ui.transcript.FALLBACK_BANNER_TEMPLATE`'s fixed
    sentence across multiple rows once the terminal narrows enough (2 rows
    at 100 columns, 4 at 40) -- a mismatch none of ``accounted_rows``, the
    fold arithmetic, or the gate's static count would catch on their own,
    since none of them read back what the banner widget actually painted.
    """
    app = _Harness()
    async with app.run_test(size=(pane_width, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entries = _fallback_entries(1, body_len=100_000)
        await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.banner is not None
        assert unit.banner.outer_size.height == 1, (
            f"pane_width={pane_width}: fallback banner painted "
            f"{unit.banner.outer_size.height} rows, not the single row RA3 "
            "and accounted_rows both assume"
        )


@pytest.mark.asyncio
async def test_a_fallback_banner_reports_the_hidden_count_and_the_total() -> None:
    """AE1 (plan docs/plans/2026-08-11-v0-3-unit-b2-fallback-banner-scope.md,
    KTD1): a 600-row fallback entry with 499 rows retained renders "101 of
    600 lines of this entry hidden" — the hidden count and the total, never
    the retained count. The old label ("clipped at the viewport edge") is a
    requirement to disappear, and its absence is asserted, not omitted.
    """
    app = _Harness()  # DEFAULT_MOUNT_CAP: the real product cap
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        body = "\n".join(f"row {i}" for i in range(600))
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=body, committed=True, line_span=(0, 600)
            ),
        )
        view = await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.is_fallback and unit.banner is not None
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1, (
            "a 600-row entry at the 500 cap retains 499 content rows"
        )
        banner = str(unit.banner.render())
        assert "101 of 600 lines of this entry hidden" in banner
        assert "clipped" not in banner, "the old label must disappear, not merely change"
        assert "at the viewport edge" not in banner, (
            "the viewport-edge phrase names the horizontal axis, not the vertical loss"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_the_banners_hidden_count_is_a_subset_of_the_condensed_prefix() -> None:
    """AE3 (plan .../2026-08-11-v0-3-unit-b2-fallback-banner-scope.md, KTD3):
    at a settled checkpoint a committed fallback entry's hidden rows are a
    subset of ``condensed_count`` — the banner's hidden count never exceeds
    the top marker's, and the two labels' scope words distinguish them.
    Here 200 one-line user rows fold entirely into the condensed prefix
    while the 600-row fallback entry retains 499, so the banner's hidden
    count (101) decomposes the prefix (301 = 101 + 200).
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        prefix = tuple(
            TranscriptEntryRecord(
                entry_id=1 + i, kind="user", raw_body="a", committed=True, line_span=(i, 1)
            )
            for i in range(200)
        )
        fallback = (
            TranscriptEntryRecord(
                entry_id=201,
                kind="assistant",
                raw_body="\n".join(f"row {i}" for i in range(600)),
                committed=True,
                line_span=(200, 600),
            ),
        )
        view = await _apply(pane, prefix + fallback)
        unit = pane._entries[201]
        assert unit.is_fallback and unit.banner is not None
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1
        hidden = 600 - len(unit.lines)
        assert hidden == 101
        assert f"{hidden} of 600 lines of this entry hidden" in str(unit.banner.render())
        assert hidden <= pane.condensed_count, (
            "the banner's hidden rows are a subset of the condensed prefix"
        )
        assert pane.condensed_count - hidden == 200, (
            "the folded ordinary prefix rows are the difference"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_descendant_triggered_fallback_retaining_every_row_renders_a_zero_banner() -> None:
    """AE5 (plan .../2026-08-11-v0-3-unit-b2-fallback-banner-scope.md, KTD2):
    a descendant-triggered fallback (a 601-column table, 1,204 descendants
    past the 1,200 trigger) retains every row and must still render a banner
    with a truthful "0 of N lines of this entry hidden" clause and the
    fallback cause. The silent case — a banner appearing without a count
    clause — is asserted not to happen.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        table = _table(1, 601)
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=table, committed=True, line_span=(0, 4)
            ),
        )
        view = await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.is_fallback and unit.banner is not None
        assert len(unit.lines) == 4, "a descendant-triggered fallback retains every row"
        banner = str(unit.banner.render())
        assert "0 of 4 lines of this entry hidden" in banner, (
            "a banner that retains every row must still state a zero hidden count"
        )
        assert "entry too large to render as markdown" in banner, (
            "the fallback cause must survive the wording change"
        )
        assert re.search(r"\d+ of \d+ lines of this entry hidden", banner) is not None, (
            "every mounted banner must carry a count clause"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_growing_fallback_tail_reuses_its_mounted_widgets() -> None:
    """A same-generation append to a fallen-back tail patches in place —
    boundary line extended, new lines mounted, banner refreshed —
    instead of dropping and rebuilding every widget. The rebuild was
    O(total lines) per delta: the full-scale gate measured the growing
    open fence's p99 apply at 17.7 s against KTD1(d)'s 50 ms ceiling.
    Widget identity is the proof of reuse — a rebuild cannot preserve it.
    The raised mount_cap keeps the tail-fold arithmetic out of this test's
    way — identity under the cap is proven separately below. The final
    banner assertion is the B2 growth-path check (plan
    docs/plans/2026-08-11-v0-3-unit-b2-fallback-banner-scope.md, AE4):
    it asserts the post-growth total on the new wording, exercising the
    KTD4 trap that the total must come from the new text, not the stale
    pre-growth ``applied_text``. At ``mount_cap=2000`` the tail never
    recycles and retained equals total, so the hidden count is 0 — which
    is exactly why this test cannot substitute for the capped-path item
    (AE4a).
    """
    app = _Harness(mount_cap=2000)
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(600))
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=0),
        )
        unit = pane._tails["assistant"]
        assert unit is not None and unit.kind == "line" and unit.is_fallback
        before = list(unit.lines)
        last_before = before[-1]

        # Extend the boundary line (no newline in the fragment) ...
        grown = fence + " extended"
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grown, generation=0),
        )
        assert pane._tails["assistant"] is unit, "the unit was rebuilt, not patched"
        assert unit.lines[-1] is last_before, "the boundary line was remounted"
        assert unit.lines[-1].source == "row 599 extended"
        assert pane.rendered_lines == view.lines[pane.condensed_count :]

        # ... then grow whole new lines and prove the prefix is untouched.
        grown2 = grown + "\n" + "\n".join(f"row {i}" for i in range(600, 700))
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grown2, generation=0),
        )
        assert pane._tails["assistant"] is unit
        assert len(unit.lines) == len(before) + 100
        assert all(a is b for a, b in zip(unit.lines, before, strict=False)), (
            "previously mounted line widgets were replaced during an append"
        )
        assert unit.banner is not None
        # Re-expressed from the old `str(len(unit.lines)) in ...` — under
        # the new wording that old assertion passed by coincidence, because
        # at mount_cap=2000 retained equals total and the total string
        # contains the retained length. This asserts the post-growth TOTAL
        # (701, from the new text) on the B2 wording, so a stale pre-growth
        # total would fail it.
        assert f"0 of {len(unit.lines)} lines of this entry hidden" in str(
            unit.banner.render()
        ), "the post-growth banner must state the hidden count and the post-growth total"
        assert unit.accounted_rows == len(unit.lines) + 1
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_capped_fallback_tail_raises_hidden_count_across_recycle_only_boundaries() -> None:
    """AE4a (plan docs/plans/2026-08-11-v0-3-unit-b2-fallback-banner-scope.md,
    KTD4): the recycle-only boundary, which every other acceptance item here
    passes with the defect present. A fallback tail grown past the DEFAULT
    cap mounts no fresh widgets on a growth boundary — the growth is served
    entirely by the recycle loop, and the banner must still refresh, because
    the total grows while the retained count is frozen. Two consecutive
    recycle-only boundaries are asserted, because a single one can pass on a
    coincidence of the arithmetic.
    """
    app = _Harness()  # DEFAULT_MOUNT_CAP: the real product cap
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        body = "\n".join(f"row {i}" for i in range(600))
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=body, generation=0),
        )
        unit = pane._tails["assistant"]
        assert unit is not None and unit.kind == "line" and unit.is_fallback
        assert unit.banner is not None
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1
        assert "101 of 600 lines of this entry hidden" in str(unit.banner.render())

        grown = body + "\n" + "\n".join(f"row {i}" for i in range(600, 700))
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grown, generation=0),
        )
        assert pane._tails["assistant"] is unit, "growth must patch, not rebuild"
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1
        assert "201 of 700 lines of this entry hidden" in str(unit.banner.render())

        grown2 = grown + "\n" + "\n".join(f"row {i}" for i in range(700, 800))
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grown2, generation=0),
        )
        assert pane._tails["assistant"] is unit
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1
        assert "301 of 800 lines of this entry hidden" in str(unit.banner.render())
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_apply_in_flight_is_visible_mid_apply_and_clear_after() -> None:
    """``apply_in_flight`` is the marker the replay gate's mid-stream
    sampler uses to tell a torn instant (Textual's Markdown.update sets
    ``source`` before its children finish remounting) from real
    corruption. It must read True inside apply()'s await window and False
    the moment apply returns — a stuck marker would excuse every
    mid-stream ownership sample, which is why the gate's settled
    checkpoint independently asserts it cleared.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        seen: list[bool] = []
        original = pane._reconcile_tail

        async def spy(kind: TranscriptKind, tail: ProvisionalTail) -> None:
            seen.append(pane.apply_in_flight)
            await original(kind, tail)

        pane._reconcile_tail = spy  # type: ignore[method-assign]
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="hello", generation=0),
        )
        assert seen and all(seen), "apply_in_flight must be True inside apply()"
        assert not pane.apply_in_flight, "apply_in_flight must clear when apply() returns"


@pytest.mark.asyncio
async def test_a_monster_fallback_tail_is_capped_and_folds_into_the_condensed_prefix() -> None:
    """The plan's "the tails, each bounded" clause, made true: a live
    fallen-back assistant tail retains at most ``mount_cap`` accounted rows
    (cap-1 content rows plus its one banner), and its folded head rows
    enter the condensed-prefix arithmetic so the settled line-window
    identity keeps holding. Before the cap, the full-scale gate's
    growing-open-fence workload mounted 10,002 line widgets for a single
    tail — the trigger only switched the rendering, it bounded nothing.
    """
    app = _Harness()  # DEFAULT_MOUNT_CAP: the real product cap
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200))
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=0),
        )
        unit = pane._tails["assistant"]
        assert unit is not None and unit.kind == "line" and unit.is_fallback
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1, (
            "partial retention keeps cap-1 content rows plus the banner"
        )
        assert unit.banner is not None
        assert pane.condensed_count == len(view.lines) - (DEFAULT_MOUNT_CAP - 1)
        assert pane.rendered_lines == view.lines[pane.condensed_count :]

        grown = fence + "\n" + "\n".join(f"row {i}" for i in range(1200, 1300))
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grown, generation=0),
        )
        assert pane._tails["assistant"] is unit, "growth must patch, not rebuild"
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1, "growth must not outgrow the cap"
        assert pane.condensed_count == len(view.lines) - (DEFAULT_MOUNT_CAP - 1)
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_monster_reasoning_tail_is_capped_without_touching_the_line_identity() -> None:
    """The reasoning tail gets the identical bound, enforced widget-locally:
    it has no span in the flattened line buffer (the projection carries only
    the assistant tail), so its folded rows must never move
    ``condensed_count`` — the cap is real but invisible to the line identity.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200))
        view = await _apply(
            pane,
            (),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text=fence, generation=0),
        )
        unit = pane._tails["reasoning"]
        assert unit is not None and unit.kind == "line" and unit.is_fallback
        assert len(unit.lines) <= DEFAULT_MOUNT_CAP
        assert pane.condensed_count == 0, "a reasoning-tail fold must not move the line prefix"
        assert pane.rendered_lines == view.lines

        grown = fence + "\n" + "\n".join(f"row {i}" for i in range(1200, 1300))
        await _apply(
            pane,
            (),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text=grown, generation=0),
        )
        assert pane._tails["reasoning"] is unit
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1, (
            "cap-1 content rows: the banner is charged within the budget (CR1 finding 6)"
        )
        assert unit.accounted_rows == DEFAULT_MOUNT_CAP
        assert pane.condensed_count == 0


@pytest.mark.asyncio
async def test_a_mixed_fill_and_recycle_append_keeps_the_painted_order() -> None:
    """One delta that both fills unused ring slots and recycles head rows:
    the fresh rows must be mounted before any head is moved, or the moved
    head lands ahead of the still-unmounted fresh rows and the painted
    order diverges from unit.lines while rendered_lines reports the
    bookkeeping order (CR1 finding 1). The pane's actual child order is the
    assertion, not the bookkeeping.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        # A 601-column table trips the fallback trigger on DESCENDANTS while
        # only a few rows tall — the one shape that yields a fallen-back
        # tail mounted well under the cap, so a later delta can both fill
        # free slots and recycle in the same apply.
        wide_row = "|" + "|".join("x" for _ in range(601)) + "|"
        table = _table(1, 601).rstrip("\n")
        grow1 = table + "\n" + "\n".join(wide_row for _ in range(400))
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=table, generation=0),
        )
        unit = pane._tails["assistant"]
        assert unit is not None and unit.is_fallback
        assert len(unit.lines) == 3  # far under the cap: nothing folded yet
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grow1, generation=0),
        )
        assert pane._tails["assistant"] is unit
        assert len(unit.lines) == 403

        grown = grow1 + "\n" + "\n".join(wide_row for _ in range(150))
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grown, generation=0),
        )
        assert pane._tails["assistant"] is unit, "growth must patch, not rebuild"
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1
        painted = [child for child in pane.children if isinstance(child, TranscriptLine)]
        assert painted == unit.lines, (
            "the mounted child order must equal the bookkeeping order"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_regenerated_short_tail_unfolds_its_predecessors_provisional_rows() -> None:
    """A monster tail folds provisional rows; a regeneration replaces it
    with a short tail and those rows leave the projection. The folded-tail
    counter must follow them back down — folding them into the monotone
    committed prefix left condensed_count larger than the projection and
    hid the final response (CR1 finding 2). Committed rows stay folded
    (nothing un-condenses); only the provisional share unfolds.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entry = TranscriptEntryRecord(
            entry_id=1, kind="assistant", raw_body="hello", committed=True, line_span=(0, 1)
        )
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200))
        view = await _apply(
            pane,
            (entry,),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=0),
        )
        assert pane.condensed_count == len(view.lines) - (DEFAULT_MOUNT_CAP - 1)
        assert pane.condensed_count > 1

        short = "done."
        view = await _apply(
            pane,
            (entry,),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=short, generation=1),
        )
        assert pane.condensed_count == 1, (
            "the committed row stays folded (monotone); every provisional row unfolds"
        )
        assert pane.condensed_count <= len(view.lines)
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_tail_ending_in_a_newline_counts_rows_the_projections_way() -> None:
    """The projection splits the streaming buffer with splitlines() — a
    trailing newline adds no row — while every piece of the pane's tail
    arithmetic used split(\"\\n\") and counted one extra (CR1 finding 4).
    A monster tail WITH a trailing newline must fold to exactly the same
    window a trailing-newline-free tail does, and the line identity holds.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200)) + "\n"
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=0),
        )
        unit = pane._tails["assistant"]
        assert unit is not None and unit.is_fallback
        assert len(unit.lines) == DEFAULT_MOUNT_CAP - 1
        assert pane.condensed_count == len(view.lines) - (DEFAULT_MOUNT_CAP - 1)
        assert pane.rendered_lines == view.lines[pane.condensed_count :]

        grown = fence + "extra 0\nextra 1\n"
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=grown, generation=0),
        )
        assert pane._tails["assistant"] is unit
        assert pane.rendered_lines == view.lines[pane.condensed_count :]
        assert pane.rendered_lines[-1] == "extra 1"


@pytest.mark.asyncio
async def test_a_session_switch_after_a_monster_tail_folded_everything_still_resets() -> None:
    """A monster tail's retention can fold EVERY committed entry, leaving
    ``_entries`` empty while ``_top`` still describes the outgoing
    session's line arithmetic. The reset check keyed on mounted entries
    alone then found nothing to compare and skipped the reset, so the
    switched-to session's first rows — line spans restarting at zero,
    under the stale ``_top`` — read as already folded and never mounted
    (CR1 confirm round). The lineage watermark closes the hole:
    ``entry_seq`` climbs across a session clear, so a swapped-in history
    can never contain the outgoing lineage's newest id.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entry = TranscriptEntryRecord(
            entry_id=1, kind="assistant", raw_body="hello", committed=True, line_span=(0, 1)
        )
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200))
        await _apply(
            pane,
            (entry,),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=0),
        )
        assert not pane._entries, "the defect's precondition: every committed entry folded away"
        assert pane._top == 1

        switched = (
            TranscriptEntryRecord(
                entry_id=2,
                kind="assistant",
                raw_body="session B's first message",
                committed=True,
                line_span=(0, 1),
            ),
        )
        view = await _apply(pane, switched)
        assert 2 in pane._entries, "the new session's entry must mount, not read as folded"
        assert pane.condensed_count == 0
        assert pane.rendered_lines == view.lines

        view = await _apply(pane, ())
        assert not pane._entries
        assert pane.condensed_count == 0
        assert pane.rendered_lines == ()


@pytest.mark.asyncio
async def test_a_late_reasoning_tail_still_paints_above_the_assistant_tail() -> None:
    """The declared tail order is reasoning above assistant (_TAIL_KINDS),
    and apply() reconciles in that order — but mounting a fresh tail by
    appending at the pane's end holds the order only when reasoning
    happens to mount first. A reasoning tail first appearing (or
    rebuilding) while the assistant tail was already on screen landed
    below it, and the commit handoff adopts tail widgets in place, so the
    screen stayed reversed against _entry_order and rendered_lines
    forever (CR2 re-review). A fresh tail must mount before the next
    tail in painted order.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)

        def tail_positions() -> tuple[int, int]:
            children = list(pane.children)
            reasoning = pane._tails["reasoning"]
            assistant = pane._tails["assistant"]
            assert reasoning is not None and assistant is not None
            return (
                children.index(reasoning.widgets()[0]),
                children.index(assistant.widgets()[0]),
            )

        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="answering", generation=0),
        )
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(
                kind="assistant", raw_text="answering more", generation=0
            ),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="thinking", generation=0),
        )
        first, second = tail_positions()
        assert first < second, "a late reasoning tail must mount above the assistant tail"

        # A generation bump to a zero-block source demotes the block tail
        # and rebuilds it as line widgets — the rebuild must re-anchor too.
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(
                kind="assistant", raw_text="answering more", generation=0
            ),
            reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="\n", generation=1),
        )
        rebuilt = pane._tails["reasoning"]
        assert rebuilt is not None and rebuilt.kind == "line"
        first, second = tail_positions()
        assert first < second, "a rebuilt reasoning tail must stay above the assistant tail"

        # Terminal commit: both tails are committed EXACTLY as they streamed
        # (the domain never rewrites reasoning at commit — CR4 finding 6),
        # so both tail widgets are adopted in place and the committed
        # painted order is whatever the tails painted — which the anchor
        # above just made correct. The reasoning body "\n" spans two
        # committed rows, so its adoption also exercises the retarget.
        committed = (
            TranscriptEntryRecord(
                entry_id=1, kind="reasoning", raw_body="\n", committed=True, line_span=(0, 2)
            ),
            TranscriptEntryRecord(
                entry_id=2,
                kind="assistant",
                raw_body="answering more",
                committed=True,
                line_span=(2, 1),
            ),
        )
        view = await _apply(pane, committed)
        children = list(pane.children)
        assert list(pane._entry_order) == [1, 2]
        assert pane._entries[1] is rebuilt, "the reasoning tail is adopted, retargeted in place"
        assert len(rebuilt.lines) == 2, "the two-row committed span, split the committed way"
        assert children.index(pane._entries[1].widgets()[0]) < children.index(
            pane._entries[2].widgets()[0]
        ), "the committed painted order must match the entry order"
        assert pane.condensed_count == 0
        assert pane.rendered_lines == view.lines


@pytest.mark.asyncio
async def test_a_committed_trailing_newline_body_never_adopts_a_short_line_tail() -> None:
    """The commit handoff adopts a line-kind tail on text equality — but the
    tail's widgets were split with splitlines() (a trailing newline adds no
    row) while the committed span counts split("\\n") rows (it adds one).
    Equal text, unequal rows: adopting the one-row-short widget list as-is
    broke ``rendered_lines == view.lines`` (CR1 finding 4's cousin on the
    adoption seam, found probing the CR2 fix). The divergence is retargeted
    in place: the widgets are reused, their sources rewritten to the
    committed rows, and the extra row mounts inside the unit.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="\n", generation=0),
        )
        tail_unit = pane._tails["assistant"]
        assert tail_unit is not None and tail_unit.kind == "line"
        assert len(tail_unit.lines) == 1, "splitlines: the trailing newline adds no tail row"

        committed = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body="\n", committed=True, line_span=(0, 2)
            ),
        )
        view = await _apply(pane, committed)
        assert view.lines == ("", "")
        unit = pane._entries[1]
        assert unit is tail_unit, "the widgets are reused — retargeted, not rebuilt"
        assert len(unit.lines) == 2, "the committed entry owns both rows of its span"
        assert pane.rendered_lines == view.lines


@pytest.mark.asyncio
async def test_a_capped_monster_tail_is_still_adopted_at_commit() -> None:
    """The adoption guard compares row conventions, not retained widgets: a
    capped monster tail retains mount_cap-1 rows, so a guard comparing
    len(tail.lines) against the committed span rejected every capped
    adoption and remounted the full body as fresh widgets at commit — a
    1,202-widget transient, the exact shape the cap exists to prevent
    (CR2 confirm round). The complete projected row count of the tail's
    text is what must equal the span; the trailing-newline divergence is
    still rejected by exactly that comparison.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200))
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=0),
        )
        tail_unit = pane._tails["assistant"]
        assert tail_unit is not None and tail_unit.kind == "line" and tail_unit.is_fallback
        assert len(tail_unit.lines) == DEFAULT_MOUNT_CAP - 1

        committed = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body=fence,
                committed=True,
                line_span=(0, 1201),
            ),
        )
        # A mount spy, not peak_mounted: that metric samples after
        # condensation and cannot see a mid-apply transient (CR4 finding 5).
        mounted_fresh: list[int] = []
        original_mount_all = pane.mount_all

        def spying_mount_all(widgets, **kwargs):  # type: ignore[no-untyped-def]
            batch = list(widgets)
            mounted_fresh.append(len(batch))
            return original_mount_all(batch, **kwargs)

        pane.mount_all = spying_mount_all  # type: ignore[method-assign]
        try:
            view = await _apply(pane, committed)
        finally:
            pane.mount_all = original_mount_all  # type: ignore[method-assign]
        assert pane._entries[1] is tail_unit, "the capped tail must be adopted, not rebuilt"
        assert sum(mounted_fresh) == 0, (
            "adoption mounts nothing fresh — no full-body widget transient at commit"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_carriage_return_body_never_adopts_the_tails_row_content() -> None:
    """splitlines() consumes a \\r\\n (and a bare \\r) as one line boundary;
    split("\\n") leaves the \\r inside the row. Equal row COUNTS, different
    row content — so a guard comparing only counts adopted the tail's
    (" ", " ") widgets under a committed view of (" \\r", " ") and broke
    the rendered-lines identity (CR2 confirm round 2). The sequences are
    compared in full, and a divergence retargets the mounted widgets to
    the committed rows in place — the painted content is the committed
    convention's, never the tail's.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        body = " \r\n "
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=body, generation=0),
        )
        tail_unit = pane._tails["assistant"]
        assert tail_unit is not None and tail_unit.kind == "line"
        assert len(tail_unit.lines) == 2, "splitlines: the \\r\\n is one boundary, two rows"

        committed = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=body, committed=True, line_span=(0, 2)
            ),
        )
        view = await _apply(pane, committed)
        assert view.lines == (" \r", " ")
        assert pane._entries[1] is tail_unit, "the widgets are reused — retargeted, not rebuilt"
        assert pane.rendered_lines == view.lines, "the painted rows are the committed rows"


@pytest.mark.asyncio
async def test_a_divergent_capped_monster_commits_without_a_mount_transient() -> None:
    """The retarget's whole reason to exist: a capped monster tail whose
    row sequences legitimately diverge (a trailing newline) used to fall
    through to an uncapped fresh build while the old tail was still
    mounted — 1,203 new widgets beside 501 existing, 1,704 before
    condensation could fold either (CR2 confirm round 3), invisible to
    peak_mounted because that metric samples after condensation. The
    commit apply must mount ZERO fresh widgets: both sides hold the
    newest rows, so the rewrite is purely positional.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200)) + "\n"
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=0),
        )
        tail_unit = pane._tails["assistant"]
        assert tail_unit is not None and tail_unit.kind == "line" and tail_unit.is_fallback
        assert len(tail_unit.lines) == DEFAULT_MOUNT_CAP - 1

        mounted_fresh: list[int] = []
        original_mount_all = pane.mount_all

        def spying_mount_all(widgets, **kwargs):  # type: ignore[no-untyped-def]
            batch = list(widgets)
            mounted_fresh.append(len(batch))
            return original_mount_all(batch, **kwargs)

        pane.mount_all = spying_mount_all  # type: ignore[method-assign]
        committed = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body=fence,
                committed=True,
                line_span=(0, 1202),
            ),
        )
        try:
            view = await _apply(pane, committed)
        finally:
            pane.mount_all = original_mount_all  # type: ignore[method-assign]
        assert pane._entries[1] is tail_unit, "the capped tail is retargeted, not rebuilt"
        assert sum(mounted_fresh) == 0, "a positional rewrite mounts nothing fresh"
        assert len(tail_unit.lines) == DEFAULT_MOUNT_CAP - 1
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_partial_fold_refreshes_the_banner_count() -> None:
    """Partial condensation trims a fallback unit's head rows, and the
    banner kept announcing the pre-fold count — "2 lines clipped" over
    one remaining row (CR5 re-review). Every other path that changes a
    fallback unit's row count (growth, retarget) refreshes the banner;
    the fold's trim loop must too. Under the B2 wording (plan
    docs/plans/2026-08-11-v0-3-unit-b2-fallback-banner-scope.md, KTD1)
    the banner reports the HIDDEN count, so a fold that trims retained
    rows from 2 to 1 raises the hidden count from 0 to 1 — the direction
    "hidden" promises (AE2), the exact inversion the old retained-count
    banner was the defect for.
    """
    app = _Harness(mount_cap=4)
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        monster = ("x" * 100_000) + "\n" + ("y" * 100_000)
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=monster, committed=True, line_span=(0, 2)
            ),
            TranscriptEntryRecord(
                entry_id=2, kind="user", raw_body="a", committed=True, line_span=(2, 1)
            ),
            TranscriptEntryRecord(
                entry_id=3, kind="user", raw_body="b", committed=True, line_span=(3, 1)
            ),
        )
        view = await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.is_fallback and unit.banner is not None
        assert len(unit.lines) == 1, "the straddle fold retained exactly one content row"
        assert "1 of 2 lines of this entry hidden" in str(unit.banner.render()), (
            "the banner must state the hidden count on the new wording through a partial fold"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_a_budget_exact_zero_block_tail_keeps_every_row() -> None:
    """The tail budget reserved a banner row unconditionally, but only
    fallback units mount a banner: a 500-row zero-block tail mounted 499
    rows, folded nothing (its accounted rows fit the cap, so nothing
    entered the condensed arithmetic), and silently dropped a row — the
    identity broke before AND after commit, the retarget slice repeating
    the same reservation (CR2 confirm round 4). A bannerless unit keeps
    the full budget at construction and at retarget.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        body = " \n" * (DEFAULT_MOUNT_CAP - 1) + " "
        view = await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=body, generation=0),
        )
        unit = pane._tails["assistant"]
        assert unit is not None and unit.kind == "line" and not unit.is_fallback
        assert unit.banner is None
        assert len(unit.lines) == DEFAULT_MOUNT_CAP, "no banner mounts, so no row is reserved"
        assert pane.condensed_count == 0
        assert pane.rendered_lines == view.lines

        # The retarget arm: the same shape ending in a newline commits as
        # exactly mount_cap split-convention rows; a banner-reserving
        # retarget slice retained one row short of the span.
        trailing = " \n" * (DEFAULT_MOUNT_CAP - 1)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text=trailing, generation=1),
        )
        rebuilt = pane._tails["assistant"]
        assert rebuilt is not None and len(rebuilt.lines) == DEFAULT_MOUNT_CAP - 1
        committed = (
            TranscriptEntryRecord(
                entry_id=1,
                kind="assistant",
                raw_body=trailing,
                committed=True,
                line_span=(0, DEFAULT_MOUNT_CAP),
            ),
        )
        view = await _apply(pane, committed)
        assert pane._entries[1] is rebuilt, "retargeted in place"
        assert len(rebuilt.lines) == DEFAULT_MOUNT_CAP, "the full span, not span minus a banner"
        assert pane.condensed_count == 0
        assert pane.rendered_lines == view.lines


@pytest.mark.asyncio
async def test_a_block_tail_completing_into_a_monster_body_demotes_at_commit() -> None:
    """message.complete may carry a body far past what streamed. Adopting
    the block document on the prefix check alone kept an uncapped,
    bannerless EntryMarkdown painting 1,339 wrapped rows as settled
    transcript — the trigger was never rechecked against the final body
    (CR3 re-review). The handoff now rechecks both demotion conditions:
    a demoting body never adopts a block tail, and its fresh build is
    pre-capped line rendering with the banner.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="hello", generation=0),
        )
        tail_unit = pane._tails["assistant"]
        assert tail_unit is not None and tail_unit.kind == "block"
        block_widget = tail_unit.block
        assert block_widget is not None

        body = "hello" + "x" * 100_000
        committed = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=body, committed=True, line_span=(0, 1)
            ),
        )
        view = await _apply(pane, committed)
        unit = pane._entries[1]
        assert unit is not tail_unit, "a demoting final body never adopts the block tail"
        assert unit.kind == "line" and unit.is_fallback
        assert unit.banner is not None
        assert len(unit.lines) == 1, "one source row, painted no-wrap and clipped"
        # Structural detachment, not is_mounted: Textual flips that flag on
        # the widget's own message pump, which can lag the awaited removal.
        assert block_widget.parent is None, "the streamed document leaves the pane"
        assert pane._tails["assistant"] is None
        assert pane.rendered_lines == view.lines

        # The pre-cap arm needs a MULTILINE demoting body: a single
        # 100,005-character line caps trivially at its one source row, so
        # removing the committed-entry mount cap would leave the arm above
        # green (CR4 finding 1). A 1,201-row fence must mount at most
        # mount_cap widgets in one batch — cap-1 content rows plus the
        # banner — observed at the mount seam, where transients live.
        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200))
        both = committed + (
            TranscriptEntryRecord(
                entry_id=2,
                kind="assistant",
                raw_body=fence,
                committed=True,
                line_span=(1, 1201),
            ),
        )
        mounted_fresh: list[int] = []
        original_mount_all = pane.mount_all

        def spying_mount_all(widgets, **kwargs):  # type: ignore[no-untyped-def]
            batch = list(widgets)
            mounted_fresh.append(len(batch))
            return original_mount_all(batch, **kwargs)

        pane.mount_all = spying_mount_all  # type: ignore[method-assign]
        try:
            view = await _apply(pane, both)
        finally:
            pane.mount_all = original_mount_all  # type: ignore[method-assign]
        monster = pane._entries[2]
        assert monster.kind == "line" and monster.is_fallback
        assert len(monster.lines) == DEFAULT_MOUNT_CAP - 1
        assert mounted_fresh and max(mounted_fresh) <= DEFAULT_MOUNT_CAP, (
            "a pre-capped demoting build mounts at most cap-1 rows plus the banner"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]

        # The same multiline commit WITH ITS BLOCK TAIL STILL LIVE — the
        # handoff frame itself. The arm above commits with no tail mounted,
        # so a mutation that caps only tail-less builds (or only tailed
        # ones) survives one arm alone; together they pin the cap on both
        # paths (CR4 confirm).
        await _apply(
            pane,
            both,
            assistant_tail=ProvisionalTail(kind="assistant", raw_text="hello2", generation=2),
        )
        live_tail = pane._tails["assistant"]
        assert live_tail is not None and live_tail.kind == "block"
        tailed_body = "hello2\n" + "\n".join(f"row {i}" for i in range(1200))
        all_three = both + (
            TranscriptEntryRecord(
                entry_id=3,
                kind="assistant",
                raw_body=tailed_body,
                committed=True,
                line_span=(1202, 1201),
            ),
        )
        mounted_fresh.clear()
        pane.mount_all = spying_mount_all
        try:
            view = await _apply(pane, all_three)
        finally:
            pane.mount_all = original_mount_all
        handoff_monster = pane._entries[3]
        assert handoff_monster is not live_tail, "a demoting final body never adopts the tail"
        assert handoff_monster.kind == "line" and handoff_monster.is_fallback
        assert len(handoff_monster.lines) == DEFAULT_MOUNT_CAP - 1
        assert mounted_fresh and max(mounted_fresh) <= DEFAULT_MOUNT_CAP, (
            "the handoff-frame demoting build is pre-capped too"
        )
        assert pane.rendered_lines == view.lines[pane.condensed_count :]


@pytest.mark.asyncio
async def test_the_demotion_frame_collection_actually_frees_the_document() -> None:
    """The demotion-frame gc.collect() exists to reclaim the destroyed
    block document inside the RA4-excluded frame — but the frame's own
    locals (the loop variable, the unit binding) still referenced the
    document at the collect, so all its widgets survived into a later
    collection that ambushed a steady-state apply (CR3 re-review). The
    removal loop now lives in its own frame and the unit binding is
    dropped first; the document must be gone the moment apply returns.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        await _apply(
            pane,
            (),
            assistant_tail=ProvisionalTail(
                kind="assistant", raw_text="a short streamed block", generation=0
            ),
        )
        unit = pane._tails["assistant"]
        assert unit is not None and unit.kind == "block" and unit.block is not None
        doc_ref = weakref.ref(unit.block)
        del unit

        fence = "```text\n" + "\n".join(f"row {i}" for i in range(1200))
        was_enabled = gc.isenabled()  # restore, don't assume (CR4 finding 7)
        gc.disable()
        try:
            await _apply(
                pane,
                (),
                assistant_tail=ProvisionalTail(kind="assistant", raw_text=fence, generation=1),
            )
            demoted = pane._tails["assistant"]
            assert demoted is not None and demoted.kind == "line"
            assert doc_ref() is None, (
                "the demotion-frame collection must free the demoted document"
            )
        finally:
            if was_enabled:
                gc.enable()


# ── condensation over mixed units (KTD2) — the four pinned regressions ─────


@pytest.mark.asyncio
async def test_aggregate_ceiling_regression_folds_on_accounted_rows() -> None:
    """302 one-line fallen-back entries: a lines-only fold (302 content
    lines, under any per-line cap of 500) would never fire, mounting 604
    widgets. Folding on accounted rows (content + banner, 2 per entry) must
    hold the folded window at <=500 accounted rows and <=600 descendants.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        entries = _fallback_entries(302)
        await _apply(pane, entries)
        assert pane.descendant_count <= 600
        accounted = sum(u.accounted_rows for u in pane._entries.values())
        assert accounted <= DEFAULT_MOUNT_CAP


@pytest.mark.asyncio
async def test_odd_cut_regression_rounds_forward_never_orphans_a_banner() -> None:
    """250 one-line fallen-back entries plus one ordinary line = 501
    accounted rows; desired_top lands inside the *oldest* entry's two-row
    span (one content row, one banner). The fold must take the whole entry,
    banner included, rather than leave a bannerless clipped row or an
    orphan banner.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        fallback = _fallback_entries(250, start_id=1)
        ordinary = TranscriptEntryRecord(
            entry_id=251, kind="tool", raw_body="ordinary", committed=True, line_span=(250, 1)
        )
        entries = fallback + (ordinary,)
        await _apply(pane, entries)

        assert 1 not in pane._entries, "the oldest fallen-back entry folds whole (rounds forward)"
        assert pane.condensed_count == 1, "exactly one real content line folded away"
        assert 2 in pane._entries
        survivor = pane._entries[2]
        assert survivor.banner is not None and len(survivor.lines) == 1, (
            "no orphan banner, no bannerless row"
        )


@pytest.mark.asyncio
async def test_partial_retention_regression_keeps_one_content_row_and_one_banner() -> None:
    """A two-line fallen-back entry (oldest) plus 498 ordinary single-line
    entries = 501 accounted rows; folding exactly one row must retain one
    content row plus exactly one banner (painted rows = 2), not round
    forward and drop the whole entry — the always-round-forward
    implementation that satisfies the odd-cut regression fails this one.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        two_line = TranscriptEntryRecord(
            entry_id=1,
            kind="assistant",
            raw_body="x" * 100_000 + "\n" + "y" * 100_000,
            committed=True,
            line_span=(0, 2),
        )
        ordinary = tuple(
            TranscriptEntryRecord(
                entry_id=2 + i,
                kind="tool",
                raw_body=f"line{i}",
                committed=True,
                line_span=(2 + i, 1),
            )
            for i in range(498)
        )
        entries = (two_line,) + ordinary
        await _apply(pane, entries)

        assert pane.condensed_count == 1
        assert 1 in pane._entries, "the entry survives with a partial retention, not a full fold"
        survivor = pane._entries[1]
        assert len(survivor.lines) == 1
        assert survivor.banner is not None
        accounted = sum(u.accounted_rows for u in pane._entries.values())
        assert accounted == DEFAULT_MOUNT_CAP


@pytest.mark.asyncio
async def test_block_rendered_newest_entry_is_mounted_whole_and_exempted() -> None:
    """A block-rendered newest entry is mounted whole regardless of the
    folded window's own budget; ordinary lines around it fold under the cap
    like any line content (KTD2's qualified newest-entry rule).
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        ordinary = tuple(
            TranscriptEntryRecord(
                entry_id=1 + i, kind="tool", raw_body=f"line{i}", committed=True, line_span=(i, 1)
            )
            for i in range(600)
        )
        block_entry = TranscriptEntryRecord(
            entry_id=601,
            kind="assistant",
            raw_body="# heading\n\nbody text",
            committed=True,
            line_span=(600, 3),
        )
        entries = ordinary + (block_entry,)
        await _apply(pane, entries)

        assert 601 in pane._entries and pane._entries[601].kind == "block"
        assert (
            pane.descendant_count <= 600 + 10
        )  # the exempted block entry's own few descendants, on top


# ── per-construct render oracles (R1) ───────────────────────────────────────


@pytest.mark.asyncio
async def test_per_construct_oracles_fail_when_flattened() -> None:
    """Heading, bullet list, ordered list, block quote, and fence each
    render as their own block class -- proven by asserting each class is
    present, which a flattened (paragraph-only) rendering could not satisfy.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)):
        pane = app.query_one("#t", TranscriptPane)
        body = (
            "# Heading\n\n"
            "- bullet one\n- bullet two\n\n"
            "1. ordered one\n2. ordered two\n\n"
            "> a quoted line\n\n"
            "```\ncode fence\n```\n"
        )
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=body, committed=True, line_span=(0, 11)
            ),
        )
        await _apply(pane, entries)
        unit = pane._entries[1]
        assert unit.kind == "block"
        doc = unit.block
        assert doc is not None
        assert list(doc.query(MarkdownH1))
        assert list(doc.query(MarkdownBulletList))
        assert list(doc.query(MarkdownOrderedList))
        assert list(doc.query(MarkdownBlockQuote))
        assert list(doc.query(MarkdownFence))
        # The failure-when-flattened half: a plain paragraph carrying the
        # same visible characters mounts none of these block classes.
        flat = "\n\n".join(
            [
                "Heading",
                "bullet one bullet two",
                "ordered one ordered two",
                "a quoted line",
                "code fence",
            ]
        )
        flat_entries = (
            TranscriptEntryRecord(
                entry_id=2, kind="assistant", raw_body=flat, committed=True, line_span=(11, 9)
            ),
        )
        await _apply(pane, entries + flat_entries)
        flat_unit = pane._entries[2]
        flat_doc = flat_unit.block
        assert flat_doc is not None
        assert not list(flat_doc.query(MarkdownH1))
        assert not list(flat_doc.query(MarkdownBulletList))
        assert not list(flat_doc.query(MarkdownBlockQuote))
        assert not list(flat_doc.query(MarkdownFence))


# ── RA2: bounded-fractional table column widths (amended R3) ───────────────

_WHITESPACE: re.Pattern[str] = re.compile(r"\s+")


def _dense(text: str) -> str:
    """``text`` with every run of whitespace collapsed away entirely."""
    return _WHITESPACE.sub("", text)


def _painted_dense_text(widget: Widget) -> str:
    """Every character actually painted for ``widget``, across every one of
    its rendered rows, whitespace collapsed away — this is the "actual
    painted content" RA2 asks the test to assert, not the un-wrapped string
    the cell was constructed with. It is robust to *where* word-wrap or
    character-fold happened to break a line (a wrap point silently eats the
    separating space either way), while still catching a real defect: a
    dropped character or an injected one — an ellipsis, for instance — would
    make this not match the source text's own dense form.
    """
    lines = [widget.render_line(y).text for y in range(widget.size.height)]
    return "".join(_dense(line) for line in lines)


def _markdown_table(headers: Sequence[str], rows: Sequence[Sequence[str]]) -> str:
    header_line = "|" + "|".join(headers) + "|"
    delim_line = "|" + "|".join("---" for _ in headers) + "|"
    row_lines = ["|" + "|".join(row) + "|" for row in rows]
    return "\n".join([header_line, delim_line, *row_lines]) + "\n"


@pytest.mark.asyncio
async def test_table_cells_paint_full_content_at_80_columns_no_truncation() -> None:
    """RA2's amended R3: every cell's actual content is legible at 80
    columns without a mouse — no ellipsis, no tooltip dependence. Asserted
    against what the cell widgets actually paint on screen, not just the
    string each was constructed with (the stock widget's cells hold the
    same full string too; it is what gets *painted* that ellipsis would
    have truncated).
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        headers = ("Name", "Description", "Status")
        rows = (
            (
                "alpha",
                "a modestly long piece of descriptive text that needs to wrap",
                "ok",
            ),
            (
                "beta",
                "short",
                "a longer status note that also has to wrap somewhere in here",
            ),
        )
        body = _markdown_table(headers, rows)
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=body, committed=True, line_span=(0, 4)
            ),
        )
        await _apply(pane, entries)
        await pilot.pause()
        await pilot.pause()

        doc = pane._entries[1].block
        assert doc is not None
        cells = list(doc.query(MarkdownTableCellContents))
        expected = [*headers, *(cell for row in rows for cell in row)]
        assert len(cells) == len(expected)
        for cell, text in zip(cells, expected, strict=True):
            assert cell.size.width >= 3, f"cell {text!r} collapsed to a degenerate width"
            assert _painted_dense_text(cell) == _dense(text), (
                f"cell {text!r} lost content instead of wrapping"
            )


@pytest.mark.asyncio
async def test_five_column_probe_no_column_starved_at_80_columns() -> None:
    """The plan's own probed failure this amendment exists to prevent:
    Textual's stock ``grid-columns: auto`` assigned this exact five-column
    shape at 80 columns the widths ``[0, 0, 58, 1, 1]`` — four columns
    starved to nothing because the fifth column's own widest cell dominated
    the auto layout with no regard for its neighbors. RA2's bounded-
    fractional algorithm must not reproduce that: every column gets a
    real, non-degenerate, actually-painted share, and the one long,
    unbreakable word character-wraps instead of starving everything else
    or silently losing characters.
    """
    app = _Harness()
    async with app.run_test(size=(80, 24)) as pilot:
        pane = app.query_one("#t", TranscriptPane)
        headers = ("A", "B", "Detail", "D", "E")
        long_cell = "x" * 70
        rows = (("p", "q", long_cell, "r", "s"),)
        body = _markdown_table(headers, rows)
        entries = (
            TranscriptEntryRecord(
                entry_id=1, kind="assistant", raw_body=body, committed=True, line_span=(0, 3)
            ),
        )
        await _apply(pane, entries)
        await pilot.pause()
        await pilot.pause()

        doc = pane._entries[1].block
        assert doc is not None
        cells = list(doc.query(MarkdownTableCellContents))
        expected = [*headers, *rows[0]]
        assert len(cells) == len(expected)
        # `cell.size` is the cell's *content* area (padding excluded) --
        # the plan's own probed defect was a literal zero there for four of
        # the five columns, not merely a narrow one; every narrow column
        # here still legitimately earns only a couple of content columns
        # once the dominant column's content length claims most of the
        # proportional remainder, so the anti-regression bar is "never
        # zero," matching the defect being prevented, not an arbitrary
        # larger floor this shape was never going to clear.
        widths = [cell.size.width for cell in cells]
        assert all(w >= 1 for w in widths), (
            f"a column was starved to a zero content width: {widths!r}"
        )
        for cell, text in zip(cells, expected, strict=True):
            assert _painted_dense_text(cell) == _dense(text), (
                f"cell {text!r} lost content — a starved column would have "
                "truncated or hidden it"
            )
