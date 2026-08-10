"""KTD1(d)'s adversarial workloads, and the latency/high-water harness that
runs them (U6, gap 2 of the 2026-08-09 results document).

**What KTD1(d) actually asks for.** Three growth shapes, exact sizes: a
growing unclosed fence fed 100 lines per boundary to 10,000 lines; a growing
table fed 10 rows per boundary to 1,000 rows; a single unbroken line grown
5,000 characters per boundary to 100,000 characters. Plus the boundary
probes named in KTD1(a)'s own grounding: the 37,000-character double-width
CJK line, the 601-column table, and the exact-boundary 599-column-table-
plus-paragraph regression. Clock: ``time.monotonic()`` around
:meth:`~talaria.ui.transcript.TranscriptPane.apply`, first 10 boundaries
excluded as warm-up, ceiling the p99 of the rest at 50ms.

**Why the growth rides the live tail, not a committed entry.** A committed
entry is append-only and immutable once mounted
(``TranscriptPane._reconcile_committed``'s own docstring: "Nothing
un-condenses it" — the same is true in the other direction, nothing
re-mounts an already-mounted entry id either), so feeding the *same*
entry id progressively larger bodies would mount it once and then silently
skip every later call. A growing delta is a streaming shape, and the pane's
own streaming shape is the live tail (:meth:`TranscriptPane._reconcile_tail`),
which appends on a same-generation growth exactly the way a real
``message.delta`` sequence does — so this module drives the assistant tail,
one constant generation, growing ``raw_text`` at every boundary. That also
means each boundary's ``apply()`` call exercises the real
``EntryMarkdown.append`` reparse path (KTD1(c)) whenever the tail is still
block-rendered, not a synthetic stand-in for it.

**The table shape is one column, not several.** KTD1(a)'s own grounding
names the number this module reproduces exactly: "the gate's 1,000-row table
workload measures 1,003 descendants, fitting with headroom" under the 1,200
per-entry trigger. 1,003 = 1,000 data cells + 1 header cell + 2 table
overhead (``_TABLE_OVERHEAD`` in ``talaria/ui/transcript.py``) — which is
only true for a *one-column* table; a wider one would blow past 1,200 long
before 1,000 rows and never reach the size the plan states.

**Reaching Textual's private ``_last_parsed_line``.** KTD1(c)'s work metric
is "parser-input bytes... the reparse window ``Markdown.append`` actually
processes, from the last unfinished top-level block to the end" — a fact
about Textual's own ``append()`` body
(``textual/widgets/_markdown.py:1460-1463`` in 8.2.8), which computes
``updated_source = "".join(self._markdown.splitlines(keepends=True)
[self._last_parsed_line:])`` and hands that to its parser. There is no
public accessor for it, and it is the one number this KTD clause is actually
about, so :func:`reparse_window_bytes` below reads the same private
attribute and mirrors the same formula rather than approximating it —
exactly the gate.py module's own established practice of reaching
implementation internals when the public surface does not expose what a
proof needs, documented at each site rather than done quietly.

**A real, pre-existing defect this module's own probing found, and does not
paper over.** Textual 8.2.8's ``Markdown.update`` (the path a widget's own
*constructor* takes to seed its initial content — ``Markdown.__init__``
calls the same code ``update()`` does) sets ``_last_parsed_line`` with a
heuristic blind to which construct is still open: "total source lines minus
one if the last line is non-empty" (``_markdown.py:1435-1436``). That is
correct for a construct that happens to be exactly one line at construction
time (this module's mega-line workload, always one line, is immune by
construction) and *wrong* for a multi-line construct that is still open — a
table with only its header/separator/first row mounted, or an unclosed
fence — because the heuristic points ``_last_parsed_line`` past the
construct's own opening lines. The next ``append()`` call then reparses a
window that starts *inside* the construct with no header or fence-open
marker in view, which markdown-it parses as a bare paragraph, and
``MarkdownTable._update_from_block``/the generic block-replace path then
silently swaps the mounted table or fence for that paragraph — probed live,
reproduced with :func:`talaria.replay.gate.block_documents_are_owned`
reporting exactly this as "projected line 0 (...) is owned by no block".
This is a fact about the installed widget's own construction-vs-append
bookkeeping, not something introduced by ``talaria/ui/blocks.py``'s
wrapping (reproduced against a bare, unwrapped stock ``textual.widgets.
Markdown`` too). It has since been fixed at both layers —
``talaria/ui/blocks.py``'s ``EntryMarkdown.update`` re-derives
``_last_parsed_line`` from the last top-level block start, and the commit
handoff no longer skips the no-op update that made the corruption permanent
— with fail-then-pass coverage in the ui suite. :func:`measure_apply_latency`
keeps its ``growth_mode="update"`` option as the harness for measuring a
full-replace (what a generation bump does once per tail replacement), but
the fence and table workloads no longer run on it: with the defect fixed,
``"append"`` measures the real construct on the real streaming path, which
is the path KTD1(d)'s per-delta ceiling is a claim about.
"""

from __future__ import annotations

import math
import time
from collections.abc import AsyncIterator, Iterator, Sequence
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from typing import Any

from textual.app import App, ComposeResult

from talaria.domain.projection import (
    EntryScopedView,
    ProvisionalTail,
    TranscriptView,
)
from talaria.ui.blocks import EntryMarkdown
from talaria.ui.literal import defang
from talaria.ui.transcript import (
    DESCENDANT_ESTIMATE_TRIGGER,
    TranscriptPane,
    descendant_estimate,
    wrapped_row_estimate,
)

# ── exact sizes, transcribed from the plan's KTD1(d) text verbatim ─────────

FENCE_LINES_PER_BOUNDARY: int = 100
FENCE_MAX_LINES: int = 10_000

TABLE_ROWS_PER_BOUNDARY: int = 10
TABLE_MAX_ROWS: int = 1_000

LINE_CHARS_PER_BOUNDARY: int = 5_000
LINE_MAX_CHARS: int = 100_000

#: The first N boundaries are warm-up (JIT/allocator settling) and excluded
#: from the p99 quantile, exactly as KTD1(d) states.
WARMUP_BOUNDARIES: int = 10

#: KTD1(d)'s latency ceiling: p99 per-boundary apply time under the 50ms
#: coalescing interval.
LATENCY_CEILING_MS: float = 50.0

#: The boundary probes, exact shapes probed live and named in KTD1(a)'s own
#: grounding comments (talaria/ui/transcript.py).
CJK_LINE_CHAR_COUNT: int = 37_000
TABLE_601_COLUMNS: int = 601
TABLE_599_COLUMNS: int = 599

#: The gate's own terminal geometry (matches ``talaria/replay/gate.py``'s
#: ``GATE_SIZE``) — not imported from there to avoid a circular import
#: (``gate.py`` imports this module), so the two are pinned equal by
#: ``tests/replay/test_gate.py`` instead of by a shared constant.
DEFAULT_WORKLOAD_SIZE: tuple[int, int] = (100, 40)

#: KTD1(d)'s "through resize including 80 columns" clause for the mega-line
#: workload.
RESIZE_WIDTH_80_COLUMNS: int = 80


# ── growth generators (KTD1(d)'s three named workloads) ────────────────────


def growing_fence_boundaries() -> Iterator[str]:
    """An unclosed fence, fed 100 lines per boundary to 10,000 lines.

    Never closed — an open fence is exactly the degenerate shape KTD1(a)'s
    grounding probes ("the 10,000-line open fence (one block, two
    descendants, 10,004 rows)"): a fence's descendant estimate is constant
    (2, ``_TWO_WIDGET_BLOCK_TYPES``), so it is the *wrapped-row* condition of
    the two-condition trigger, not the descendant one, that must fire once
    the source line count clears :data:`~talaria.ui.transcript.
    DEFAULT_MOUNT_CAP` — this is the shape that proves the wrapped-row half
    of the trigger is load-bearing, not redundant with the descendant half.
    """
    lines: list[str] = []
    n = 0
    while n < FENCE_MAX_LINES:
        add = min(FENCE_LINES_PER_BOUNDARY, FENCE_MAX_LINES - n)
        lines.extend(f"line {n + i}" for i in range(add))
        n += add
        yield "```python\n" + "\n".join(lines)


def growing_table_boundaries() -> Iterator[str]:
    """A one-column table, fed 10 rows per boundary to 1,000 rows.

    One column: see the module docstring — this is the exact shape that
    keeps the full-size table's descendant estimate at 1,003, fitting under
    the 1,200 trigger "with headroom", which is precisely what this workload
    exists to hold.
    """
    header = "| col |\n| --- |"
    rows: list[str] = []
    n = 0
    while n < TABLE_MAX_ROWS:
        add = min(TABLE_ROWS_PER_BOUNDARY, TABLE_MAX_ROWS - n)
        rows.extend(f"| r{n + i} |" for i in range(add))
        n += add
        yield header + "\n" + "\n".join(rows)


def growing_line_boundaries() -> Iterator[str]:
    """A single unbroken line, grown 5,000 characters per boundary to 100,000.

    The wrapped-rows degenerate shape: one source line, so the descendant
    estimate never moves (a paragraph is always 1 descendant), and only the
    width-aware wrapped-row estimate can ever trip the fallback trigger —
    which a character-count estimate would catch too late relative to what
    the widget actually paints (KTD1(a)'s own double-width finding).
    """
    n = 0
    while n < LINE_MAX_CHARS:
        n += min(LINE_CHARS_PER_BOUNDARY, LINE_MAX_CHARS - n)
        yield "x" * n


# ── boundary probes (KTD1(a)'s own grounding, reproduced exactly) ──────────


def cjk_double_width_line(char_count: int = CJK_LINE_CHAR_COUNT) -> str:
    """The 37,000-character double-width CJK line boundary probe.

    ``"端"`` — the same double-width character ``talaria/replay/stress.py``
    already draws from for its own width-awareness coverage — measures 2
    display cells per :func:`rich.cells.cell_len`, which is the whole point:
    a character-count estimate would undercount this line's wrapped rows by
    up to half (KTD1(a)'s own probed fact).
    """
    return "端" * char_count


def table_n_columns(n: int, *, rows: int = 1) -> str:
    """An ``n``-column table with a fixed row count — the 601-column probe.

    ``rows=1`` by default to match KTD1(a)'s own grounding exactly: "a
    probed three-line, 601-column table (601 header cells + 601 body cells)
    mounts 1,204 descendants" — three markdown lines (header, separator, one
    data row), 601 + 601 = 1,202 cells, plus the table's own 2-descendant
    overhead.
    """
    header = "| " + " | ".join(f"c{i}" for i in range(n)) + " |"
    sep = "| " + " | ".join("---" for _ in range(n)) + " |"
    body_row = "| " + " | ".join(f"v{i}" for i in range(n)) + " |"
    body = "\n".join(body_row for _ in range(rows))
    return f"{header}\n{sep}\n{body}"


def table_599_columns_plus_paragraph() -> str:
    """The exact-boundary regression: a 599-column table plus one paragraph.

    KTD1(a)'s own grounding: "(599 + 599) + 2 (table) + 1 (paragraph) =
    1,201, exactly" — the calibrated estimate must exceed 1,200 and fire the
    trigger, and 1,201 does. Two data rows (not one), matching the same
    grounding's own worked arithmetic (599 header cells + 599 body cells).
    """
    return table_n_columns(TABLE_599_COLUMNS, rows=1) + "\n\none trailing paragraph"


# ── the harness: real TranscriptPane, real EntryMarkdown.append ────────────


@dataclass
class LatencyReport:
    """One workload's per-boundary latencies plus KTD1's high-water figures."""

    label: str
    boundary_count: int = 0
    samples_ms: list[float] = field(default_factory=list)
    #: KTD1(a) tier-two: peak descendants of the single live tail this
    #: harness grows. Tier-one (the folded window) is proven separately by
    #: the real-fold regressions already in ``tests/replay/test_gate.py``
    #: (the 302-entry aggregate ceiling, the odd-cut pair) — this harness
    #: mounts no committed entries at all, so it has no folded window to
    #: measure and does not claim to cover that tier.
    peak_descendants: int = 0
    #: KTD1(c): the largest single reparse window handed to
    #: ``EntryMarkdown.append`` across every boundary, in UTF-8 bytes,
    #: computed by :func:`reparse_window_bytes`.
    peak_parser_input_bytes: int = 0
    #: KTD1(b): the tallest the live tail's mounted widget(s) painted, in
    #: rows, across every boundary.
    tallest_document_rows: int = 0
    #: True once any boundary tripped the two-condition fallback trigger
    #: (KTD1(a)) — expected and correct for all three growth workloads. The
    #: one-column table's descendant estimate stays at 1,003, under the
    #: 1,200 trigger "with headroom" exactly as the plan states — but the
    #: *other* half of the trigger still fires: 1,000 rows is 1,002 source
    #: lines regardless of column width, and the wrapped-row estimate counts
    #: at least one row per source line, clearing DEFAULT_MOUNT_CAP (500) on
    #: line count alone. So this is True for the table too — the plan's own
    #: parenthetical is a claim about the descendant number specifically,
    #: proving that metric alone is not conservative enough on its own, not
    #: a claim that this workload avoids the trigger altogether.
    fell_back: bool = False
    #: The static construct-aware estimate (``descendant_estimate`` /
    #: ``wrapped_row_estimate``) at the *final* boundary, computed
    #: independently of what actually got mounted. Recorded because a
    #: boundary that trips the fallback trigger is demoted to line rendering
    #: *within the same* ``apply()`` call, before this harness ever reads
    #: ``pane.descendant_count`` — so for the boundary probes, whose whole
    #: point is proving the estimate exceeds 1,200 and the trigger correctly
    #: fires, ``peak_descendants`` alone would only ever show the safe,
    #: post-fallback count. This is the number the plan's own regressions are
    #: actually stated over.
    final_descendant_estimate: int = 0
    final_wrapped_row_estimate: int = 0
    #: RA4: the boundary index at which the tail first demoted to fallback
    #: line rendering — the one apply() per workload that swaps a monster
    #: block document for a capped run of line widgets in a single call.
    #: ``None`` when the workload never fell back.
    demotion_boundary: int | None = None
    #: RA4: that boundary's own apply latency, reported as high-water
    #: instrumentation precisely *because* :meth:`p99_ms` excludes it.
    demotion_apply_ms: float = 0.0

    @property
    def peak_apply_ms(self) -> float:
        return max(self.samples_ms, default=0.0)

    @property
    def p99_ms(self) -> float:
        """Nearest-rank p99 over the steady-state phase: every boundary past
        :data:`WARMUP_BOUNDARIES` and, when the workload demoted, past the
        demotion boundary too (RA4 + RA5).

        With ~90 post-warmup samples, nearest-rank p99 is arithmetically the
        maximum — so the quantile as originally stated demanded that the
        one-time representation switch (mounting a capped run of line widgets
        in one apply) cost under 50 ms, which no amount of steady-state work
        can deliver. RA4 excluded that single flagged boundary, reported
        verbatim in :attr:`demotion_apply_ms`. RA5 (operator-decided
        2026-08-09, on the third full-scale run) narrows enforcement to the
        post-demotion phase entirely: a still-open, block-rendered table
        re-renders wholesale on every append, crossing the ceiling at ~340
        rows (54–59 ms plateau, 190 ms outlier observed) until the 500-row
        trigger demotes it — a real, recorded limit
        (:attr:`block_phase_peak_ms`), not an enforced one; the early
        demotion of open tables is queued follow-up. Zero for a run too
        short to have any measured samples at all — never a fabricated
        number, matching this package's own "never claim a measurement you
        did not take" discipline (``gate.py``'s ``MIN_RSS_SAMPLES``/
        ``MIN_CONTENT_CHECKPOINTS`` are the same instinct for a different
        measurement).
        """
        start = WARMUP_BOUNDARIES
        if self.demotion_boundary is not None:
            start = max(start, self.demotion_boundary + 1)
        measured = sorted(self.samples_ms[start:])
        if not measured:
            return 0.0
        index = max(0, math.ceil(0.99 * len(measured)) - 1)
        return measured[min(index, len(measured) - 1)]

    @property
    def block_phase_peak_ms(self) -> float:
        """Peak apply latency of the block-rendered phase — post-warm-up
        samples strictly before the demotion boundary. RA5's recorded limit:
        reported everywhere, enforced nowhere. Zero when the workload never
        demoted or its block phase ended inside warm-up.
        """
        if self.demotion_boundary is None:
            return 0.0
        return max(self.samples_ms[WARMUP_BOUNDARIES : self.demotion_boundary], default=0.0)

    def to_dict(self) -> dict[str, Any]:
        return {
            "label": self.label,
            "boundary_count": self.boundary_count,
            "measured_samples": max(0, len(self.samples_ms) - WARMUP_BOUNDARIES),
            "p99_ms": round(self.p99_ms, 3),
            "peak_apply_ms": round(self.peak_apply_ms, 3),
            "peak_descendants": self.peak_descendants,
            "peak_parser_input_bytes": self.peak_parser_input_bytes,
            "tallest_document_rows": self.tallest_document_rows,
            "fell_back": self.fell_back,
            "final_descendant_estimate": self.final_descendant_estimate,
            "final_wrapped_row_estimate": self.final_wrapped_row_estimate,
            "demotion_boundary": self.demotion_boundary,
            "demotion_apply_ms": round(self.demotion_apply_ms, 3),
            "block_phase_peak_ms": round(self.block_phase_peak_ms, 3),
        }


def reparse_window_bytes(widget: EntryMarkdown, fragment: str) -> int:
    """Mirror ``Markdown.append``'s own reparse-window computation exactly
    (see the module docstring), so this is the same number Textual's own
    ``append()`` is about to hand its parser for ``fragment`` — not a guess.
    """
    defanged = defang(fragment)  # EntryMarkdown.append defangs before appending
    next_markdown = widget.source + defanged
    updated_source = "".join(next_markdown.splitlines(keepends=True)[widget._last_parsed_line :])
    return len(updated_source.encode("utf-8"))


@asynccontextmanager
async def _pane_harness(*, size: tuple[int, int]) -> AsyncIterator[tuple[Any, TranscriptPane]]:
    class _WorkloadHarness(App[None]):
        def compose(self) -> ComposeResult:
            yield TranscriptPane(id="t")

    app = _WorkloadHarness()
    async with app.run_test(size=size) as pilot:
        yield pilot, app.query_one(TranscriptPane)


def _tail_rendered_rows(pane: TranscriptPane) -> int:
    """Sum of painted rows for whatever is currently mounted as the
    assistant live tail — the single unit this harness ever grows.
    """
    unit = pane._tails.get("assistant")
    if unit is None:
        return 0
    return sum(max(1, widget.outer_size.height) for widget in unit.widgets())


async def measure_apply_latency(
    boundaries: Sequence[str],
    *,
    label: str,
    size: tuple[int, int] = DEFAULT_WORKLOAD_SIZE,
    resize_at: Sequence[tuple[int, int, int]] = (),
    growth_mode: str = "append",
) -> LatencyReport:
    """Drive the assistant live tail through every boundary of ``boundaries``,
    timing each :meth:`TranscriptPane.apply` call with ``time.monotonic()``
    (KTD1(d)'s clock) and recording KTD1's high-water figures.

    ``resize_at`` is a ``(boundary_index, width, height)`` triple sequence:
    immediately before the boundary at that index, the harness resizes the
    terminal and pauses for the layout to settle — KTD1(d)'s "through
    resize, including 80 columns" clause for the mega-line workload.

    ``growth_mode`` — ``"append"`` (the default) keeps one constant stream
    generation across every boundary, so :meth:`TranscriptPane._reconcile_tail`
    takes the same incremental ``EntryMarkdown.append`` path a real streaming
    delta sequence does, exercising KTD1(c)'s reparse-window claim.
    ``"update"`` bumps the generation every boundary instead, forcing a full
    replace (``EntryMarkdown.update``) each time.

    The fence and table workloads use ``"update"`` for a discovered, real
    reason, not a convenience — see the module docstring's "A real,
    pre-existing defect" section: appending to an already-mounted multi-line
    construct does not reliably keep it that construct in this installed
    Textual version, so ``"update"`` is what lets those two workloads measure
    the real fence/table's cost rather than a paragraph's.
    """
    report = LatencyReport(label=label)
    resize_map = {index: (width, height) for index, width, height in resize_at}
    previous_text = ""
    async with _pane_harness(size=size) as (pilot, pane):
        for index, text in enumerate(boundaries):
            if index in resize_map:
                width, height = resize_map[index]
                await pilot.resize_terminal(width, height)
                await pilot.pause()

            generation = index if growth_mode == "update" else 0

            if growth_mode == "append":
                tail_unit = pane._tails.get("assistant")
                if (
                    tail_unit is not None
                    and tail_unit.kind == "block"
                    and text.startswith(previous_text)
                    and tail_unit.block is not None
                ):
                    fragment = text[len(previous_text) :]
                    if fragment:
                        window = reparse_window_bytes(tail_unit.block, fragment)
                        report.peak_parser_input_bytes = max(
                            report.peak_parser_input_bytes, window
                        )

            view = TranscriptView(lines=tuple(text.split("\n")), entry_count=0)
            entries = EntryScopedView(
                entries=(),
                assistant_tail=ProvisionalTail(
                    kind="assistant", raw_text=text, generation=generation
                ),
                reasoning_tail=ProvisionalTail(kind="reasoning", raw_text="", generation=0),
            )

            start = time.monotonic()
            await pane.apply(view, entries)
            elapsed_ms = (time.monotonic() - start) * 1000.0

            report.samples_ms.append(elapsed_ms)
            report.boundary_count += 1
            report.peak_descendants = max(report.peak_descendants, pane.descendant_count)
            # Outside the timed region on purpose: `outer_size.height` needs
            # Textual's own arrange pass to have settled, which `apply()`
            # does not itself await -- pausing here to read it accurately
            # would corrupt the KTD1(d) clock, which is stated as
            # "around TranscriptPane.apply" specifically.
            await pilot.pause()
            report.tallest_document_rows = max(
                report.tallest_document_rows, _tail_rendered_rows(pane)
            )
            unit = pane._tails.get("assistant")
            if unit is not None and unit.is_fallback:
                if not report.fell_back:
                    # RA4: the first fallen-back boundary is the demotion —
                    # the one representation-switch apply the quantile
                    # excludes and this report surfaces verbatim.
                    report.demotion_boundary = index
                    report.demotion_apply_ms = elapsed_ms
                report.fell_back = True
            report.final_descendant_estimate = descendant_estimate(text)
            report.final_wrapped_row_estimate = wrapped_row_estimate(
                text, content_width=pane._content_width
            )

            previous_text = text
    return report


async def measure_single_shot(
    text: str, *, label: str, size: tuple[int, int] = DEFAULT_WORKLOAD_SIZE
) -> LatencyReport:
    """One boundary, for the probes that are a fixed shape rather than a
    growth curve (the CJK line, the 601/599-column tables).
    """
    return await measure_apply_latency((text,), label=label, size=size)


#: The three KTD1(d) growth workloads plus the boundary probes, named so
#: ``gate.py`` can run the whole set and record every label without
#: duplicating this module's own shape knowledge.
GROWTH_WORKLOADS: tuple[tuple[str, Any], ...] = (
    ("growing-open-fence", growing_fence_boundaries),
    ("growing-one-column-table", growing_table_boundaries),
    ("growing-unbroken-line", growing_line_boundaries),
)

BOUNDARY_PROBES: tuple[tuple[str, str], ...] = (
    ("cjk-37000-double-width-line", cjk_double_width_line()),
    ("table-601-columns", table_n_columns(TABLE_601_COLUMNS)),
    ("table-599-columns-plus-paragraph", table_599_columns_plus_paragraph()),
)


@dataclass
class WorkloadResults:
    """Every KTD1(d) workload's report, plus the arithmetic verdict."""

    reports: tuple[LatencyReport, ...]
    latency_ceiling_ms: float = LATENCY_CEILING_MS
    descendant_trigger: int = DESCENDANT_ESTIMATE_TRIGGER

    @property
    def passed(self) -> bool:
        return all(report.p99_ms <= self.latency_ceiling_ms for report in self.reports)

    def to_dict(self) -> dict[str, Any]:
        return {
            "latency_ceiling_ms": self.latency_ceiling_ms,
            "descendant_trigger": self.descendant_trigger,
            "pass": self.passed,
            "reports": [report.to_dict() for report in self.reports],
        }


async def run_adversarial_workloads(
    *, size: tuple[int, int] = DEFAULT_WORKLOAD_SIZE
) -> WorkloadResults:
    """Run every KTD1(d) growth workload and boundary probe, once.

    The mega-line workload is the one KTD1(d) names "through resize" — it
    resizes to :data:`RESIZE_WIDTH_80_COLUMNS` a third of the way through its
    own growth and continues, so the latency samples after that boundary are
    measured at 80 columns, not only at the harness's own starting width.
    """
    reports: list[LatencyReport] = []
    for label, factory in GROWTH_WORKLOADS:
        boundaries = list(factory())
        resize_at: tuple[tuple[int, int, int], ...] = ()
        if label == "growing-unbroken-line":
            resize_at = ((len(boundaries) // 3, RESIZE_WIDTH_80_COLUMNS, 24),)
        # Every growth workload measures "append" — the path a real
        # streaming delta sequence takes (KTD3 drives Markdown.append), and
        # the path KTD1(d)'s per-delta ceiling is a claim about. The fence
        # and table originally forced "update" because the
        # construction-vs-append defect the module docstring names made
        # append silently swap those constructs for a paragraph; that
        # defect is fixed (EntryMarkdown.update's checkpoint correction and
        # the unconditional commit repair), so append now measures the real
        # fence and table. "update" — a full re-render of the whole tail
        # every boundary — is what a generation bump does once per
        # replacement, not per delta, and holding a per-delta ceiling over
        # it measured a path streaming never takes.
        report = await measure_apply_latency(
            boundaries, label=label, size=size, resize_at=resize_at, growth_mode="append"
        )
        reports.append(report)
    for label, text in BOUNDARY_PROBES:
        reports.append(await measure_single_shot(text, label=label, size=size))
    return WorkloadResults(reports=tuple(reports))
