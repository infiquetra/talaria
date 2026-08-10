"""The framework validation gate: run the real interface, record the numbers.

This module does not decide anything. It replays a corpus through the shipped
:class:`~talaria.ui.app.TalariaApp`, records KTD14's four measurements, and
compares each against its threshold. The verdict is arithmetic — the plan is
explicit that subjective smoothness is not a pass condition and that no
reviewer, panel, or observer sign-off gates it.

**What each measurement is, and what it is not.**

``peak_mounted_widgets``
    The high-water mark of live line widgets, read from the pane's own counter
    rather than sampled. A sampler can step over a spike; a counter cannot.

``rss_series`` / ``rss_slope_per_1k_frames``
    ``ru_maxrss`` sampled every :data:`RSS_SAMPLE_EVERY` frames. This is a
    *maximum* resident set, so it is monotonic by construction — it can never
    show memory being returned. That is precisely why KTD14 asks for the series
    and the fitted slope rather than a single endpoint: the endpoint answers
    "did this run stay under 300 MB", and only the slope answers "what happens
    to a session that runs all day". The slope is the input to whether domain
    transcript eviction becomes a milestone-3 requirement.

``render_ticks_per_second``
    Counted in the coalescing flush callback, divided by the run's wall clock.
    With a 50ms coalescing boundary the ceiling is 20/s by construction, so this
    measurement is really a *check that coalescing is engaged* — a regression
    that rendered per frame would show up here as hundreds per second.

``content_loss``
    At each checkpoint, every line of every committed domain transcript entry
    must appear, in order, in the projection the UI renders from. This is the
    one measurement that would catch the worst possible outcome: a fast,
    bounded, smooth interface that is quietly dropping the conversation.
"""

from __future__ import annotations

import asyncio
import hashlib
import platform
import resource
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Final

from textual.containers import Horizontal
from textual.widgets._markdown import MarkdownBlock, MarkdownFence

from talaria.domain.models import ConnectionStatus, TerminalCause, TranscriptKind
from talaria.domain.projection import (
    EntryScopedView,
    ProvisionalTail,
    TranscriptView,
    entry_scoped_view,
    transcript_view,
)
from talaria.domain.state import SessionState, cancel_turn
from talaria.replay.controls import MAX_SPEED, MIN_SPEED, ReplayControls
from talaria.replay.source import ReplaySource, SidebandAction, load_frame_records, load_header
from talaria.replay.stress import (
    FeatureCorpus,
    StressCorpus,
    build_feature_corpus,
    build_stress_corpus,
)
from talaria.replay.workloads import WARMUP_BOUNDARIES, WorkloadResults, run_adversarial_workloads
from talaria.transport.source import FrameRecord
from talaria.ui.app import TalariaApp
from talaria.ui.blocks import EntryMarkdown, parser_factory
from talaria.ui.transcript import (
    DEFAULT_MOUNT_CAP,
    MARKDOWN_KINDS,
    TranscriptPane,
    is_zero_block,
    trips_fallback_trigger,
)


class GateError(Exception):
    """The gate could not run as specified.

    Raised rather than degrading quietly. A gate that measures fewer things than
    it claims and still exits 0 is worse than one that refuses to run.
    """


# ── KTD14 thresholds (PC7) ───────────────────────────────────────────────

#: Mounted line widgets allowed at any point of the stress corpus.
MOUNTED_WIDGET_CEILING = 600

#: KTD1(a)'s *second* ceiling — descendants, not top-level widgets. A table
#: mounts one widget per cell (``talaria/ui/blocks.py``'s module docstring),
#: so a folded window that stays under :data:`MOUNTED_WIDGET_CEILING` at the
#: top level can still have blown far past a descendant budget underneath a
#: single top-level ``MarkdownTable``. Same numeric ceiling as the top-level
#: one — KTD1(a) states one number for the folded window either way it is
#: counted — but measured against :attr:`~talaria.ui.transcript.TranscriptPane
#: .descendant_count`'s own peak, not :attr:`~talaria.ui.transcript
#: .TranscriptPane.peak_mounted`, which cannot see a per-cell spike.
DESCENDANT_WIDGET_CEILING = 600

#: Resident-set growth allowed across a full 50k-delta replay, in megabytes.
RSS_GROWTH_CEILING_MB = 300.0

#: Coalescing flushes per second allowed at maximum replay speed.
RENDER_TICKS_PER_SECOND_CEILING = 25.0

#: How often the memory series is sampled, in frames.
RSS_SAMPLE_EVERY = 5_000

#: Minimum points required before a fitted slope is worth publishing. The
#: sampler advances past every boundary crossed within one 20ms poll, so on a
#: fast machine the series can collapse to two points — an endpoint difference
#: reported as a fitted slope. Below this the gate fails rather than publishing
#: a number it did not really measure.
MIN_RSS_SAMPLES = 6

#: Minimum content checkpoints. "0 of 1" and "0 of 11" are very different
#: claims, and only the second is evidence.
MIN_CONTENT_CHECKPOINTS = 5

#: Terminal geometry the gate renders at. Fixed so a measurement taken on one
#: machine means the same thing on another.
GATE_SIZE = (100, 40)

#: How long the sustained-streaming pass should run. KTD14 states the render
#: tick threshold "divided by a fixed 60-second wall-clock window", and this is
#: that window.
CADENCE_WINDOW_SECONDS = 60.0


@dataclass(frozen=True)
class CorpusIdentity:
    """How a corpus is cited. Never by path — this repository is public (R29)."""

    label: str
    sha256: str
    frame_count: int
    kind: str
    note: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "label": self.label,
            "sha256": self.sha256,
            "frame_count": self.frame_count,
            "kind": self.kind,
            "note": self.note,
        }


@dataclass
class GateMeasurement:
    """One replay's numbers."""

    corpus: CorpusIdentity
    frames_applied: int
    render_ticks: int
    elapsed_seconds: float
    render_ticks_per_second: float
    peak_mounted_widgets: int
    condensed_lines: int
    #: KTD1(a)'s second ceiling's high-water mark — descendants, not
    #: top-level widgets (see :data:`DESCENDANT_WIDGET_CEILING`). Defaulted
    #: rather than positional so the existing hand-built ``GateMeasurement``
    #: fixtures in ``tests/replay/test_gate.py`` (predating this field) keep
    #: constructing without naming every field.
    peak_descendant_widgets: int = 0
    rss_series_mb: list[tuple[int, float]] = field(default_factory=list)
    rss_growth_mb: float = 0.0
    rss_slope_mb_per_1k_frames: float = 0.0
    content_loss_checkpoints: int = 0
    #: Mid-stream checkpoints at which the pane actually went quiescent and
    #: the ownership proofs actually RAN — counted separately from
    #: ``content_loss_checkpoints`` because a checkpoint whose bounded wait
    #: never saw quiescence skips ownership, and a stream busy enough to
    #: defeat every wait would otherwise satisfy the checkpoint floor with
    #: zero progressive proofs behind it (CR6).
    ownership_checkpoints: int = 0
    content_loss_failures: int = 0
    transcript_entries: int = 0
    transcript_lines: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "corpus": self.corpus.to_dict(),
            "frames_applied": self.frames_applied,
            "render_ticks": self.render_ticks,
            "elapsed_seconds": round(self.elapsed_seconds, 3),
            "render_ticks_per_second": round(self.render_ticks_per_second, 3),
            "peak_mounted_widgets": self.peak_mounted_widgets,
            "peak_descendant_widgets": self.peak_descendant_widgets,
            "condensed_lines": self.condensed_lines,
            "rss_series_mb": [[frames, round(mb, 2)] for frames, mb in self.rss_series_mb],
            "rss_growth_mb": round(self.rss_growth_mb, 2),
            "rss_slope_mb_per_1k_frames": round(self.rss_slope_mb_per_1k_frames, 4),
            "content_loss_checkpoints": self.content_loss_checkpoints,
            "ownership_checkpoints": self.ownership_checkpoints,
            "content_loss_failures": self.content_loss_failures,
            "transcript_entries": self.transcript_entries,
            "transcript_lines": self.transcript_lines,
        }


@dataclass
class GateResult:
    """Every measurement, every threshold check, and the arithmetic verdict."""

    stress: GateMeasurement
    cadence: GateMeasurement
    live: GateMeasurement | None
    #: None when no live corpus was replayed, so an unmeasured run cannot
    #: be mistaken for a passing one.
    determinism_identical: bool | None
    inert_controls_refused: tuple[str, ...]
    matrix: dict[str, str]
    checks: dict[str, dict[str, Any]] = field(default_factory=dict)
    #: U6, gap 4: the scripted feature corpus's own measurement — None only
    #: if a caller constructs a GateResult by hand without running it (every
    #: real run_gate() call populates it; there is no "skip the feature
    #: corpus" mode, unlike live_corpus which is genuinely optional).
    feature: GateMeasurement | None = None
    #: U6, gap 1 (AE2 at gate level): full domain-state equality across the
    #: sideband-bearing feature corpus, replayed at three transport
    #: treatments — the same claim `determinism_identical` makes for the
    #: live corpus, extended to include the scripted sideband actions.
    sideband_determinism_identical: bool | None = None
    #: U6, gap 3 (R12): the additional normalized-block-structure comparison
    #: over the same three sideband-bearing replays.
    sideband_structure_identical: bool | None = None
    #: U6, gap 2: every KTD1(d) adversarial workload's latency/high-water
    #: report.
    workloads: WorkloadResults | None = None

    @property
    def passed(self) -> bool:
        return all(check["pass"] for check in self.checks.values())

    @property
    def verdict(self) -> str:
        return "pass" if self.passed else "fail"

    def to_dict(self) -> dict[str, Any]:
        return {
            "verdict": self.verdict,
            "matrix": self.matrix,
            "checks": self.checks,
            "stress": self.stress.to_dict(),
            "cadence": self.cadence.to_dict(),
            "live": self.live.to_dict() if self.live is not None else None,
            "feature": self.feature.to_dict() if self.feature is not None else None,
            "determinism_identical": self.determinism_identical,
            "sideband_determinism_identical": self.sideband_determinism_identical,
            "sideband_structure_identical": self.sideband_structure_identical,
            "inert_controls_refused": list(self.inert_controls_refused),
            "workloads": self.workloads.to_dict() if self.workloads is not None else None,
        }


# ── measurement helpers ──────────────────────────────────────────────────


def _rss_mb() -> float:
    """Maximum resident set, in megabytes.

    ``ru_maxrss`` is bytes on Darwin and kilobytes on Linux — the one platform
    difference in this module, and getting it wrong would silently misreport by
    a factor of 1024, so it is branched on explicitly rather than guessed from
    the magnitude.
    """
    raw = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    divisor = 1024 * 1024 if sys.platform == "darwin" else 1024
    return raw / divisor


def _fit_slope(series: list[tuple[int, float]]) -> float:
    """Least-squares slope in MB per 1,000 frames. Zero for a degenerate series."""
    if len(series) < 2:
        return 0.0
    xs = [frames / 1000.0 for frames, _ in series]
    ys = [mb for _, mb in series]
    n = float(len(series))
    mean_x = sum(xs) / n
    mean_y = sum(ys) / n
    denominator = sum((x - mean_x) ** 2 for x in xs)
    if denominator == 0.0:
        return 0.0
    numerator = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys, strict=True))
    return numerator / denominator


# ── U6: the two-part ownership proof ─────────────────────────────────────
#
# `interface_shows_everything`'s original claim (kept below, now `_line_
# window_ownership`) was "one projected line, one on-screen widget" — true
# for v0.1's flat line pane, false the moment U4 made a committed assistant
# or reasoning entry mount as one `EntryMarkdown` document instead of one
# `TranscriptLine` per line. That claim's own reconstruction still holds
# *in aggregate* (`TranscriptPane.rendered_lines` correctly rebuilds a block
# unit's contribution from its applied source text — U4's own docstring, and
# `tests/ui/test_transcript_blocks.py` pins the identity at unit scale), so
# Part 1's line-window half is retained unchanged for line-rendered content
# and for the pane-wide "everything is accounted for" arithmetic. What is
# new is the second, stronger half: that a mounted block document's own
# *blocks* — not just its flattened text — genuinely cover the projected
# lines they claim to, and are the right constructs, not merely the right
# character count.
#
# Two parts, matching the KTD8 branch-hold instruction exactly:
#
#   (1) mounted-window ownership — every projected line inside a block
#       entry's mounted window is covered by that entry's own document
#       blocks' `source_range` spans, with the blank-line accounting rule
#       stated and tested below (`_block_ownership`); applied to every
#       currently mounted `EntryMarkdown` — committed entries and both live
#       tails alike, since `TranscriptPane.query(EntryMarkdown)` does not
#       distinguish the two and neither does this proof.
#
#   (2) condensed-range accounting — folded units are accounted by their
#       line spans in the banner arithmetic, which is exactly the pane-wide
#       "mounted plus condensed equals total" identity Part 1's aggregate
#       half already proves; RA3's fallback-banner charge
#       (`_fallback_banner_accounting`) is the one place that arithmetic has
#       a chrome row (the banner) folded into it deliberately, so it gets
#       its own proof rather than trusting the aggregate identity to notice
#       a missing or doubled banner by coincidence.

#: The markdown-it token types that ever become a **top-level** child of a
#: mounted `EntryMarkdown` document — probed against Textual 8.2.8's own
#: `Markdown._parse_markdown`: a token is yielded to the document (rather
#: than appended to an ancestor's `_blocks`) exactly when it closes back to
#: an empty construction stack, which is exactly `token.level == 0` for a
#: block-opening or self-contained token. Table rows/cells, list items, and
#: blockquote-nested paragraphs all have `level > 0` and are deliberately
#: excluded — they are never one of `widget.children`, only descendants of
#: one.
_TOP_LEVEL_BLOCK_TOKENS: Final[frozenset[str]] = frozenset(
    {
        "heading_open",
        "hr",
        "paragraph_open",
        "blockquote_open",
        "bullet_list_open",
        "ordered_list_open",
        "table_open",
        "fence",
        "code_block",
    }
)


@dataclass(frozen=True)
class _ExpectedBlock:
    """One top-level construct :func:`parser_factory` would produce, ahead
    of asking Textual to mount anything — the semantic oracle's ground
    truth."""

    construct_type: str
    class_name: str
    start: int
    end: int


def _expected_top_level_blocks(source: str) -> list[_ExpectedBlock]:
    """Re-parse ``source`` with the exact parser every entry document
    parses with (KTD4 isolation — a fresh parser, no shared state) and
    return the top-level construct sequence a correctly mounted document
    must match: class, order, and span, one entry per :data:`
    _TOP_LEVEL_BLOCK_TOKENS` token at nesting level 0.
    """
    tokens = parser_factory().parse(source)
    expected: list[_ExpectedBlock] = []
    for token in tokens:
        if token.level != 0 or token.type not in _TOP_LEVEL_BLOCK_TOKENS:
            continue
        block_name = token.tag if token.type == "heading_open" else token.type
        block_cls = EntryMarkdown.BLOCKS[block_name]
        start, end = token.map if token.map is not None else (0, 0)
        expected.append(_ExpectedBlock(token.type, block_cls.__name__, start, end))
    return expected


def _construct_oracle(
    construct_type: str, block: MarkdownBlock, lines: list[str]
) -> tuple[bool, str]:
    """Per-construct visual oracle (R1): does this mounted block's actual
    rendered content trace back to its own source span, or did the
    construct get flattened/dropped/hidden on the way to the screen?

    Scoped deliberately: this proves *non-emptiness traceable to the
    source*, not full structural equality for every construct — the fence
    oracle is exact (verbatim code, byte for byte) and the table oracle is
    exact (cell count), because Textual exposes both precisely; heading,
    paragraph, block quote, and both list types are proven by "the
    construct produced content/descendants proportional to a non-blank
    span", which is weaker but still fails hard on the mutation this unit
    asks for: a hidden construct that mounts its container with nothing
    inside it.
    """
    start, end = block.source_range
    span_text = "\n".join(lines[start:end])
    if construct_type in ("fence", "code_block"):
        if not isinstance(block, MarkdownFence):
            return False, f"{construct_type} block is a {type(block).__name__}, not MarkdownFence"
        # The fence body is every span line strictly between the two ``` (or
        # indented-code) delimiter lines for a *closed* `fence`; a
        # `code_block` (an indented code block, no delimiters) is the whole
        # span verbatim. An *unclosed* fence (R14's own adversarial shape,
        # and exactly what a confirmed-cancel or typed-disconnect mid-fence
        # commits — U6's sideband scenarios) has no closing delimiter line at
        # all, so slicing off "the first and last line" as delimiters
        # (this function's own earlier shape) silently ate one real content
        # line every time the fence never closed — found live, reproduced by
        # U6's mid-fence cancellation scenario, a false failure this
        # function was producing against a widget that was rendering
        # correctly. markdown-it's own `fence` token already knows the
        # difference (its `.content` is the body either way, delimiter
        # stripping included, closed or not) — so this re-parses `span_text`
        # with the identical parser rather than hand-rolling delimiter
        # arithmetic that has to get the closed/unclosed distinction right
        # twice.
        if construct_type == "fence":
            span_tokens = parser_factory().parse(span_text)
            fence_tokens = [t for t in span_tokens if t.type == "fence"]
            if len(fence_tokens) != 1:
                return False, (
                    f"fence at lines {(start, end)}: span {span_text!r} does not "
                    f"re-parse to exactly one fence token ({len(fence_tokens)} found) "
                    "— the span itself is wrong, not just the mounted content"
                )
            expected_body = fence_tokens[0].content
        else:
            expected_body = span_text
        if block.code.rstrip("\n") != expected_body.rstrip("\n"):
            return False, (
                f"fence at lines {(start, end)}: mounted code {block.code!r} does not match "
                f"source body {expected_body!r} — dropped or substituted text"
            )
        return True, ""
    if construct_type == "table_open":
        from textual.widgets._markdown import MarkdownTableCellContents

        expected_tokens = parser_factory().parse(span_text)
        expected_cells = sum(
            1 for t in expected_tokens if t.type in ("th_open", "td_open")
        )
        actual_cells = len(list(block.query(MarkdownTableCellContents)))
        if actual_cells != expected_cells:
            return False, (
                f"table at lines {(start, end)}: mounted {actual_cells} cells, "
                f"source parses to {expected_cells} — a dropped or hidden cell"
            )
        return True, ""
    if construct_type in ("bullet_list_open", "ordered_list_open"):
        expected_tokens = parser_factory().parse(span_text)
        expected_items = sum(1 for t in expected_tokens if t.type == "list_item_open")
        actual_rows = len(list(block.query(Horizontal)))
        if actual_rows != expected_items:
            return False, (
                f"list at lines {(start, end)}: mounted {actual_rows} item rows, "
                f"source parses to {expected_items} — a dropped or hidden item"
            )
        return True, ""
    # heading, paragraph, blockquote, hr: "non-empty content traceable to a
    # non-blank span" — hr has no textual content to lose, so it is exempt.
    if construct_type == "hr":
        return True, ""
    content_plain = getattr(block, "_content", None)
    has_content = bool(content_plain is not None and content_plain.plain.strip())
    has_descendants = bool(list(block.walk_children()))
    non_blank_source = bool(span_text.strip("#> \t"))
    if non_blank_source and not (has_content or has_descendants):
        return False, (
            f"{construct_type} at lines {(start, end)}: source {span_text!r} is non-blank but "
            f"the mounted {type(block).__name__} shows nothing — a hidden construct"
        )
    return True, ""


def _prove_span_coverage(
    lines: list[str], spans: list[tuple[str, tuple[int, int]]]
) -> tuple[bool, str]:
    """Part 1's span-coverage and blank-line-accounting half, over a plain
    ``(class_name, source_range)`` sequence rather than live Textual widgets.

    Split out from :func:`document_ownership` deliberately: this is the
    exact logic a "dropped text" or "hidden construct" mutation has to defeat,
    and testing it against a hand-built ``spans`` list — rather than only
    against a real mounted widget tree, which is awkward to corrupt safely —
    is what lets a mutation test prove the proof can fail on those two
    mutation classes directly, without needing to reach into Textual's own
    object graph to simulate them.

    **Blank-line accounting rule, stated exactly as probed**: parsing
    ``"a\\n\\n# h\\n"`` yields two blocks with ``source_range`` ``(0, 1)``
    and ``(2, 3)`` — line offset 1, the blank line between them, is inside
    neither span. That is not a gap in the proof; it is accounted to the
    entry's whole span rather than to any one block. The rule this function
    enforces: every projected line offset is owned by exactly one span, *or*
    is blank *to the parser* — the empty string or a whitespace-only line,
    since CommonMark (and this document's own ``parser_factory``) treats
    ``"  "`` between two blocks exactly as it treats ``""``: neither opens a
    construct, so neither is ever claimed by a block's ``source_range``.
    Checking only the literal ``""`` rejected a whitespace-only inter-block
    line as "dropped text" even though the parser itself never disagreed
    that the line was blank. An offset that is neither owned nor blank by
    this (parser-matching) definition is exactly what "dropped text" looks
    like, and fails here.
    """
    total = len(lines)
    covered = [False] * total
    for class_name, (start, end) in spans:
        if start < 0 or end > total or start > end:
            return False, (
                f"block {class_name} source_range {(start, end)} is out of "
                f"bounds for a {total}-line document"
            )
        for offset in range(start, end):
            if covered[offset]:
                return False, (
                    f"projected line {offset} is claimed by more than one mounted block"
                )
            covered[offset] = True

    for offset, owned in enumerate(covered):
        if not owned and lines[offset].strip() != "":
            return False, (
                f"projected line {offset} ({lines[offset]!r}) is owned by no block and is "
                "not blank — the blank-line accounting rule does not excuse it"
            )
    return True, ""


def document_ownership(
    widget: EntryMarkdown, *, applied_text: str | None = None
) -> tuple[bool, str]:
    """The two-part proof's Part 1, for one mounted block document.

    Self-contained against the widget's own ``source`` — the exact
    (defanged) text it was last built or updated from, per Textual's own
    ``Markdown.source`` — so this needs no external record of which entry
    or tail the widget belongs to; the claim is "this document's own blocks
    cover this document's own text", checked the same way for a committed
    entry and for a still-streaming tail alike.

    That self-contained claim is deliberately *not* the whole proof: nothing
    above stops ``widget.source`` itself from having been swapped for
    different, internally-consistent text — every span still covers it,
    every construct oracle still holds, and this function would still
    report success on a document showing the wrong words. ``applied_text``,
    when given, is the domain-sourced text this document was last built or
    updated *from* — :class:`~talaria.ui.transcript.TranscriptPane`'s own
    per-unit record (``_MountedUnit.applied_text``), captured at the same
    call site that wrote to the widget — and is checked against
    ``widget.source`` before anything else: a mismatch means the mounted
    document no longer shows what the pane believes it told it to show.
    """
    source = widget.source
    if applied_text is not None and source != applied_text:
        return False, (
            f"widget.source {source!r} does not match the document's own applied "
            f"text {applied_text!r} — the document was swapped for different "
            "text after being built or updated, even though its own blocks are "
            "internally consistent with what it now shows"
        )
    lines = source.split("\n")
    children = [child for child in widget.children if isinstance(child, MarkdownBlock)]

    span_ok, span_reason = _prove_span_coverage(
        lines, [(type(child).__name__, child.source_range) for child in children]
    )
    if not span_ok:
        return False, span_reason

    # The semantic oracle: the parsed construct sequence must match the
    # mounted sequence exactly (class, order, span), and each construct's
    # own oracle must hold. Span coverage alone can look complete even when
    # a block was mounted as the *wrong* class (two adjacent blocks whose
    # combined span is right but whose shared boundary moved) or when a
    # construct's content was silently dropped inside an otherwise correctly
    # spanned block — this is what construct-specific oracles hunt for.
    expected = _expected_top_level_blocks(source)
    # KTD2's mounting rule says a zero-block source is never mounted as an
    # EntryMarkdown at all -- it line-renders instead (block_documents_are_
    # owned's own docstring says so). That was asserted, never verified: a
    # live tail that collapses from real content to a bare newline stays
    # mounted as a block document (probed live -- TranscriptPane._reconcile_
    # tail's block-kind branch never re-checks is_zero_block after a
    # generation change, only trips_fallback_trigger), and a document with
    # zero blocks trivially "covers" its own zero-length span, so span
    # coverage and the oracle loop below would both pass vacuously. A
    # zero-block document reaching this function at all is exactly that
    # violation, regardless of how it got here.
    if not expected:
        return False, (
            f"document parses to zero top-level blocks (source {source!r}) — KTD2's "
            "rule is that a zero-block source is never mounted as a block document "
            "at all, it line-renders instead; a mounted EntryMarkdown that collapsed "
            "to zero blocks was left in place rather than demoted"
        )
    if len(expected) != len(children):
        return False, (
            f"source parses to {len(expected)} top-level constructs, "
            f"{len(children)} are mounted"
        )
    for exp, child in zip(expected, children, strict=True):
        if type(child).__name__ != exp.class_name:
            return False, (
                f"block at lines {(exp.start, exp.end)} mounted as "
                f"{type(child).__name__}, expected {exp.class_name} for token "
                f"{exp.construct_type!r} — a wrong block class"
            )
        if child.source_range != (exp.start, exp.end):
            return False, (
                f"block {type(child).__name__} source_range {child.source_range} != "
                f"expected {(exp.start, exp.end)}"
            )
        ok, reason = _construct_oracle(exp.construct_type, child, lines)
        if not ok:
            return False, reason
    return True, ""


def block_documents_are_owned(pane: TranscriptPane) -> tuple[bool, str]:
    """Part 1 over every currently mounted block document.

    Walks :attr:`TranscriptPane._entries` and :attr:`TranscriptPane._tails`
    rather than ``pane.query(EntryMarkdown)`` — a deliberate, documented
    exception to this module's usual preference for public state (contrast
    :func:`fallback_banner_accounting`, which reaches only ``pane.children``):
    no public widget API carries an entry's identity or the text the pane
    last applied to it, and :func:`document_ownership`'s cross-check against
    that applied text (the fix for the self-referential "swapped text"
    finding) needs exactly that pairing. This still covers a committed
    entry's document and a live tail's identically — ``_entries`` and
    ``_tails`` are the pane's whole record of what is mounted as a block
    document, tail or not. Zero-block sources are proven never to reach here
    as a *checked* fact now, inside :func:`document_ownership` itself, not
    merely asserted in this docstring.
    """
    for entry_id, unit in pane._entries.items():
        if unit.kind != "block":
            continue
        if unit.block is None:
            # _MountedUnit's own invariant is that kind == "block" implies a
            # mounted block widget; a violation is a bookkeeping defect this
            # proof reports rather than crashes on.
            return False, f"entry {entry_id}: kind is 'block' but no block widget is mounted"
        ok, reason = document_ownership(unit.block, applied_text=unit.applied_text)
        if not ok:
            return False, f"entry {entry_id}: {reason}"
    for tail_kind, tail_unit in pane._tails.items():
        if tail_unit is None or tail_unit.kind != "block":
            continue
        if tail_unit.block is None:
            return False, f"{tail_kind} tail: kind is 'block' but no block widget is mounted"
        ok, reason = document_ownership(tail_unit.block, applied_text=tail_unit.applied_text)
        if not ok:
            return False, f"{tail_kind} tail: {reason}"
    return True, ""


def _ktd2_selects_block(kind: TranscriptKind, text: str, *, content_width: int) -> bool:
    """Whether KTD2's mounting rule would block-render this text.

    The identical decision :meth:`TranscriptPane._prepare_unit` makes
    (``kind in MARKDOWN_KINDS`` and not zero-block and does not trip the
    fallback trigger), applied here to the domain's own current text rather
    than trusted from the pane's choice — this is what lets
    :func:`expected_block_documents_are_mounted` notice when the pane never
    got the chance to make that choice at all.
    """
    return (
        kind in MARKDOWN_KINDS
        and not is_zero_block(text)
        and not trips_fallback_trigger(text, content_width=content_width)
    )


def expected_block_documents_are_mounted(
    app: TalariaApp, *, entries_view: EntryScopedView | None = None
) -> tuple[bool, str]:
    """The other half of mounted-window ownership: is everything that
    *should* be a block document actually mounted as one?

    ``entries_view`` selects the ground truth. The settled checkpoint omits
    it and derives the expected set from the domain's live state — the pane
    has caught up, so the stronger comparison is exact. A mid-stream caller
    passes the pane's own last-applied snapshot instead
    (:attr:`~talaria.ui.transcript.TranscriptPane.last_applied_entries`):
    mid-stream the live domain is a moving projection an unknown number of
    flushes ahead of the pane, and deriving the expected set from it fails
    a correct pane for not having flushed yet, while the snapshot is
    exactly what the pane just reconciled (CR6 — without a mid-stream run
    of this proof, an eligible tail wrongly line-rendered throughout
    streaming and repaired before settlement passed every checkpoint).

    :func:`block_documents_are_owned` only walks what is currently mounted —
    if a delta never reaches :meth:`TranscriptPane.apply` at all (a
    reasoning-only update that the app's render path drops before the
    pane's own reconciliation runs, say), there is nothing missing *from*
    that walk for it to notice: the absent document is accepted vacuously,
    the same hole the original one-line-one-widget claim had for a pane
    rendering nothing at all.

    This derives the expected set independently, from the domain's own
    current :func:`~talaria.domain.projection.entry_scoped_view` — both
    provisional tails included, per KTD8's branch instruction on this unit —
    using :func:`_ktd2_selects_block`, and fails when an entry or tail that
    rule selects for block rendering, and that is still inside the pane's
    visible window (its line span not yet folded below
    :attr:`~talaria.ui.transcript.TranscriptPane.condensed_count`), is
    absent from the mounted window.
    """
    pane = app.transcript
    if entries_view is None:
        entries_view = entry_scoped_view(app.state)
    width = pane._content_width

    for record in entries_view.entries:
        start, count = record.line_span
        if start + count <= pane.condensed_count:
            continue  # folded away -- no longer part of the visible window
        if not _ktd2_selects_block(record.kind, record.raw_body, content_width=width):
            continue
        unit = pane._entries.get(record.entry_id)
        if unit is None or unit.kind != "block":
            return False, (
                f"entry {record.entry_id} parses to a block document under KTD2's "
                "mounting rule and is still inside the pane's visible window, but "
                "is absent from the mounted window — a delta that never reached "
                "TranscriptPane.apply"
            )

    tail_kinds: tuple[tuple[TranscriptKind, ProvisionalTail], ...] = (
        ("reasoning", entries_view.reasoning_tail),
        ("assistant", entries_view.assistant_tail),
    )
    for kind, tail in tail_kinds:
        if tail.is_empty:
            continue
        if not _ktd2_selects_block(tail.kind, tail.raw_text, content_width=width):
            continue
        tail_unit = pane._tails.get(kind)
        if tail_unit is None or tail_unit.kind != "block":
            return False, (
                f"the {kind} tail parses to a block document under KTD2's mounting "
                "rule, but is absent from the mounted window — a delta that never "
                "reached TranscriptPane.apply"
            )
    return True, ""


def fallback_banner_accounting(pane: TranscriptPane) -> tuple[bool, str]:
    """RA3's banner proof, read from the mounted DOM alone.

    No private pane state: walks ``pane.children`` (public) and finds every
    maximal run of consecutive ``.transcript--nowrap`` line widgets — a
    fallen-back entry's content rows, painted one line per widget. Each such
    run must be followed **immediately** by exactly one
    ``.transcript--fallback-banner`` widget: that is what "painted rows
    exactly projected lines + 1" means at the DOM level (the run's own
    length is the projected-line count; the banner is the ``+ 1``), and what
    "a banner never stands alone" means (a banner with no run in front of it
    would be counted among mounted banners but not among proven ones, and
    the final tally below would disagree). Clipped-cell reachability is
    deliberately not claimed anywhere in this function, matching RA3's own
    text.

    The "+1" in that formula is a claim about *painted rows*, not mounted
    widgets, and counting one banner widget as one row was never actually
    checked against what the widget paints: :attr:`Widget.outer_size` (the
    same measure :meth:`TranscriptPane._condense` already reads for
    scroll-anchor accounting) is read here for exactly that reason. A
    banner's own message text is a fixed template
    (``FALLBACK_BANNER_TEMPLATE``) wider than an ordinary terminal width, so
    without a no-wrap/width constraint on the banner widget it wraps to more
    than one row — probed live: a 100,000-character fallback entry mounts a
    banner painting 2 rows at this module's own :data:`GATE_SIZE` (100
    columns). KTD1(a)'s exact-row formula is violated whenever that happens,
    and nothing about "one widget" being mounted would show it.
    """
    children = list(pane.children)
    index = 0
    proven_banners = 0
    while index < len(children):
        widget = children[index]
        if "transcript--nowrap" in widget.classes:
            run_start = index
            while index < len(children) and "transcript--nowrap" in children[index].classes:
                index += 1
            run_length = index - run_start
            next_is_banner = (
                index < len(children)
                and "transcript--fallback-banner" in children[index].classes
            )
            if not next_is_banner:
                return False, (
                    f"a fallen-back run of {run_length} content rows at pane child "
                    f"{run_start} has no banner immediately after it"
                )
            banner = children[index]
            banner_height = banner.outer_size.height
            if banner_height > 1:
                return False, (
                    f"the banner at pane child {index} paints {banner_height} rows, not "
                    "exactly one — KTD1(a)'s exact-row formula (painted rows == projected "
                    "lines + one banner row) requires the banner itself to be exactly one "
                    "painted row"
                )
            proven_banners += 1
            index += 1  # past the banner just proven
            continue
        index += 1
    total_banners = sum(1 for w in children if "transcript--fallback-banner" in w.classes)
    if total_banners != proven_banners:
        return False, (
            f"{total_banners} banner widgets are mounted but only {proven_banners} sit "
            "directly after a fallback content run — a banner standing alone, or a run "
            "the fold rule left uncharged"
        )
    return True, ""


def _line_window_ownership(app: TalariaApp, *, settled: bool) -> bool:
    """The retained half of the original one-claim proof.

    Still true and still needed: :attr:`TranscriptPane.rendered_lines`
    correctly reconstructs a block unit's contribution (its own applied
    source text, split on newline) alongside every line-rendered unit's
    literal ``.source`` — so the pane-wide "on-screen window matches the
    projection, and mounted plus condensed equals the whole" identity is
    still exactly the right proof for (a) every line-rendered surface
    (ordinary kinds, zero-block markdown, fallen-back entries) and (b) the
    aggregate arithmetic Part 2 (condensed-range accounting) rests on. What
    it is *not* any longer, on its own, is proof that a block-rendered
    entry's internal structure is correct — :func:`block_documents_are_owned`
    is what proves that half now.
    """
    view = transcript_view(app.state)
    pane = app.transcript
    on_screen = tuple(pane.rendered_lines)
    if len(on_screen) > len(view.lines):
        return False

    # Correctness, checked always: whatever the pane is showing must be a real
    # contiguous window of the projection, not invented or reordered text.
    # Anchored at the pane's own top index rather than at the tail, because
    # mid-stream the pane is legitimately behind — it renders on a coalescing
    # boundary, so between flushes it holds an older, shorter window. Requiring
    # it to match the newest tail at every instant is a race, not a defect.
    top = pane.condensed_count
    window = tuple(view.lines[top : top + len(on_screen)])
    if not settled:
        # Mid-stream this asserts liveness, not equality. The pane renders on a
        # coalescing boundary, and an arbitrary number of deltas can land
        # between two flushes — each appending lines and rewriting the tail in
        # place — so the pane is legitimately behind by an unknown number of
        # lines, and *any* suffix comparison against the current projection is a
        # race. Asserting equality here failed all eleven sustained checkpoints
        # against a pane that was provably correct once settled.
        #
        # What still holds while the stream moves: the pane cannot be showing
        # more than exists. That is a true invariant at every instant, so it is
        # asserted at every instant.
        #
        # Deliberately not asserting "the pane is non-blank" here as well. It
        # reads like a stronger check but it is a race — before the first flush
        # the projection legitimately has content the pane has not rendered yet,
        # which failed exactly one checkpoint per run — and it catches nothing
        # the settled check below does not already catch outright: a pane that
        # renders nothing fails the settled sum, since 0 mounted plus 0
        # condensed cannot equal a non-empty transcript.
        return len(on_screen) + pane.condensed_count <= len(view.lines)

    if on_screen != window:
        return False

    # Completeness, checked only once the stream has stopped and a flush has
    # run. Now there is no in-flight snapshot to be behind, so every line must
    # be either mounted or accounted for by the condensed block. This is the
    # half that a pane rendering nothing at all fails.
    return len(on_screen) + pane.condensed_count == len(view.lines)


def ownership_report(app: TalariaApp, *, settled: bool) -> tuple[bool, tuple[str, ...]]:
    """The full two-part ownership proof, with reasons a mutation test can read.

    Part 1 (mounted-window ownership) runs at *every* checkpoint, settled or
    not — a block document's own internal consistency (its blocks correctly
    span its own current text) is a true invariant at every instant, not a
    race against a moving projection the way the line-window comparison is,
    so it is not deferred to the settled checkpoint the way that half still
    is. RA3's banner accounting is likewise checked at every checkpoint: a
    fallen-back entry's banner is chrome mounted alongside its content in
    the same reconciliation pass, never after it.
    """
    reasons: list[str] = []
    pane = app.transcript

    block_ok, block_reason = block_documents_are_owned(pane)
    if not block_ok:
        reasons.append(f"mounted-window ownership: {block_reason}")

    expected_ok, expected_reason = expected_block_documents_are_mounted(app)
    if not expected_ok:
        reasons.append(f"mounted-window ownership (expected document absent): {expected_reason}")

    banner_ok, banner_reason = fallback_banner_accounting(pane)
    if not banner_ok:
        reasons.append(f"RA3 banner accounting: {banner_reason}")

    if not _line_window_ownership(app, settled=settled):
        reasons.append(
            "line-window ownership: the on-screen window does not match the projection, "
            "or mounted plus condensed does not equal the projection's total line count"
        )

    return (not reasons), tuple(reasons)


def interface_shows_everything(app: TalariaApp, *, settled: bool) -> bool:
    """Is the conversation actually on screen, as opposed to merely in the domain?

    :func:`content_is_complete` is a domain-side check, and at both of its call
    sites the ``view`` argument was ``transcript_view(app.state)`` — a pure
    function of the very state being walked. Its own docstring warns that
    comparing the projection against itself "would pass no matter what", which
    is precisely what the call sites did. Making ``TranscriptPane.apply`` a
    no-op, so the interface rendered nothing at all, produced a completely blank
    screen and a ``pass`` verdict with zero content loss.

    This is the two-part ownership proof (U6), composed by
    :func:`ownership_report` and collapsed to a bool here for the existing
    call sites: (1) mounted-window ownership — a block-rendered entry's own
    document blocks genuinely cover the lines they claim to, blank-line
    accounting and per-construct semantic oracles included — plus the
    retained line-window comparison for line-rendered surfaces and the
    pane-wide totals; (2) condensed-range accounting, including RA3's
    fallback-banner charge. A pane that renders nothing fails on the
    line-window half; a pane that renders block-shaped garbage now fails on
    the mounted-window half, which the original one-line-one-widget claim
    could not see at all.
    """
    ok, _ = ownership_report(app, settled=settled)
    return ok


def content_is_complete(state: SessionState, view: TranscriptView) -> bool:
    """Every committed entry's lines appear, in order, in the rendered projection.

    Deliberately not ``view == transcript_view(state)``: that would compare the
    projection against itself and pass no matter what. This walks the domain's
    own entries and requires each of their lines to be findable in sequence.

    Matching is by whole line, not by substring. Substring containment had two
    holes: appending junk to every rendered line still contained the original,
    and an *empty* entry text produced an empty fragment, which is ``in`` every
    string — so an empty-text entry matched whatever line the cursor happened to
    be on and advanced past it. Each entry's expected lines are rendered through
    the projection one entry at a time, which applies the same speaker prefix the
    real projection applies without this function having to know what it is.
    """
    cursor = 0
    lines = view.lines
    for entry in state.transcript:
        for expected in transcript_view(SessionState(transcript=(entry,))).lines:
            while cursor < len(lines) and lines[cursor] != expected:
                cursor += 1
            if cursor >= len(lines):
                return False
            cursor += 1
    return True


def live_corpus_identity(path: str | Path, records: tuple[FrameRecord, ...]) -> CorpusIdentity:
    """Cite a recorded corpus by digest and count. The path never leaves this call."""
    digest = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    header = load_header(path)
    note = f"frame-log v{header.version}, recorded {header.started_at}"
    return CorpusIdentity(
        label=f"talaria-live-v1-{len(records)}f-{digest[:12]}",
        sha256=digest,
        frame_count=len(records),
        kind="recorded-session",
        note=note,
    )


def stress_corpus_identity(corpus: StressCorpus) -> CorpusIdentity:
    return CorpusIdentity(
        label=corpus.label,
        sha256=corpus.sha256,
        frame_count=corpus.frame_count,
        kind="synthetic-stress",
        note=(
            f"generated, seed {corpus.seed}: {corpus.delta_count} deltas across "
            f"{corpus.turn_count} turns, with interleaved malformed frames, "
            "unknown event types and sub-agent fan-out"
        ),
    )


def feature_corpus_identity(corpus: FeatureCorpus) -> CorpusIdentity:
    return CorpusIdentity(
        label=corpus.label,
        sha256=corpus.sha256,
        frame_count=corpus.frame_count,
        kind="synthetic-feature",
        note=(
            "scripted, deterministic: every R1 construct, early termination by "
            "cancel/error/typed-disconnect (mid-table included), parser attacks, "
            "one representative of every R7 kind group, sideband timeline included"
        ),
    )


# ── U6: the sideband timeline, applied (gap 1) ───────────────────────────
#
# Confirmed-cancel and typed-disconnect are not wire frames (see
# talaria/replay/source.py's own module section); this is where the two
# scripted action kinds actually touch domain state, mirroring the one real
# call path each has live. A typed disconnect goes through the exact same
# public method the live wiring uses (TalariaApp.note_connection_state,
# wired from LiveSource.bind in talaria/cli.py) — full fidelity, nothing
# reached into. A confirmed cancel has no such live-mode equivalent to call:
# TalariaApp.action_interrupt is refused in replay mode by design (AE11 —
# there is no gateway to interrupt), and that refusal is correct for "the
# operator pressed interrupt during this replay", which is not what a
# sideband cancel means. It means the *history being replayed* already
# contains a confirmed cancellation, so this calls the domain transition
# directly (cancel_turn) — the same function interrupt_live() itself calls
# on a real confirmation — and sets `_dirty` by hand, the one place this
# module reaches an underscore-prefixed TalariaApp attribute, for the same
# reason `document_ownership` reaches TranscriptPane._entries: no public
# surface does this, and it is exactly the fact this proof needs.


def _sideband_status_for_cause(cause: TerminalCause) -> ConnectionStatus:
    """Mirrors talaria/transport/source.py's own mapping: every terminal
    cause reports ``"disconnected"`` except ``auth_failed``, which is its
    own connection status (R35's four-state contract has no fifth member for
    it — see ``ConnectionStatus``'s own docstring).
    """
    return "auth_failed" if cause == "auth_failed" else "disconnected"


def _apply_sideband_action(app: TalariaApp, action: SidebandAction) -> None:
    if action.kind == "confirmed_cancel":
        app.state = cancel_turn(app.state, at=app.state.last_observed_at)
        app._dirty = True
        return
    if action.cause is None:
        # SidebandAction.__post_init__ guarantees a typed_disconnect action
        # always carries a cause -- reaching this means that guarantee broke,
        # which is a defect in this module's own construction, not a replay
        # outcome to report gracefully.
        raise ValueError(f"typed_disconnect action {action!r} has no cause")
    app.note_connection_state(_sideband_status_for_cause(action.cause), cause=action.cause)


# ── U6: replay determinism over normalized block structure (gap 3) ───────
#
# AE11's existing check (replay_determinism, below) compares full domain
# SessionState equality — the strongest claim, and it stays. R12 additionally
# asks for a claim stated over *block structure*: ordered block classes,
# source ranges, and semantic content, with runtime identifiers excluded.
# Derived from the final SessionState alone (no live pane needed) by
# re-parsing every committed block-eligible entry's own raw body with the
# same construct-detection rule the pane itself applies
# (_ktd2_selects_block) and the same parser _expected_top_level_blocks
# already uses for the ownership proof — entry ids are deliberately absent
# from the tuple this returns, which is what "runtime identifiers excluded"
# means: two replays that commit the same content in the same order produce
# the same normalized structure even if internal bookkeeping differs.
#: The content width R12's comparison is stated under — the gate's own
#: pinned terminal geometry (GATE_SIZE), matching TranscriptPane._content_width's
#: own four-column padding subtraction.
_NORMALIZED_STRUCTURE_WIDTH: Final[int] = GATE_SIZE[0] - 4

NormalizedBlock = tuple[TranscriptKind, str, int, int, str]


def normalized_block_structure(state: SessionState) -> tuple[NormalizedBlock, ...]:
    """R12's additional replay-determinism comparison: ordered block classes,
    source ranges, and semantic content, runtime identifiers excluded,
    sideband included (a committed entry produced by cancel_turn or a typed
    disconnect is an ordinary transcript entry — it flows through
    entry_scoped_view like any other, so no special-casing is needed for the
    sideband to be "included").
    """
    entries_view = entry_scoped_view(state)
    out: list[NormalizedBlock] = []
    for record in entries_view.entries:
        if not _ktd2_selects_block(
            record.kind, record.raw_body, content_width=_NORMALIZED_STRUCTURE_WIDTH
        ):
            continue
        lines = record.raw_body.split("\n")
        for block in _expected_top_level_blocks(record.raw_body):
            span_text = "\n".join(lines[block.start : block.end])
            out.append((record.kind, block.class_name, block.start, block.end, span_text))
    return tuple(out)


# ── the replay runs ──────────────────────────────────────────────────────


async def measure_replay(
    records: tuple[FrameRecord, ...],
    identity: CorpusIdentity,
    *,
    mount_cap: int = DEFAULT_MOUNT_CAP,
    timeout: float = 900.0,
    speed: float | None = None,
    sideband: tuple[SidebandAction, ...] = (),
) -> tuple[GateMeasurement, SessionState, tuple[str, ...]]:
    """Replay one corpus through the real app and measure it.

    ``speed=None`` means unbounded — no inter-frame delay at all, which is what
    KTD14 calls maximum replay speed. A finite ``speed`` replays on the recorded
    cadence scaled by that multiplier, which is the only way to hold the
    renderer under sustained streaming long enough to measure it: at unbounded
    speed the domain reducer drains a 50,000-frame corpus in a few seconds, so
    the render loop is never the thing under load.

    ``sideband`` is U6's scripted, non-wire-frame action track (confirmed
    cancels, typed disconnects). Applied by :func:`_apply_sideband_action`,
    fired by :class:`~talaria.replay.source.ReplaySource` itself at the exact
    frame index each action targets — see that module's own section for why
    this is deterministic rather than a race against the sampler below. The
    app has to exist before the callback that reaches into it can be built,
    so the source is constructed first with no sideband, then re-armed once
    ``app`` exists — the callback itself is never invoked until the
    ``async for`` loop inside ``app.run_test()`` starts pulling frames.
    """
    controls = ReplayControls()
    if speed is None:
        controls.set_unbounded()
    else:
        controls.set_speed(speed)
    source = ReplaySource(records, controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls, mount_cap=mount_cap)
    if sideband:
        source.bind_sideband(sideband, lambda action: _apply_sideband_action(app, action))

    rss_series: list[tuple[int, float]] = [(0, _rss_mb())]
    checkpoints = 0
    ownership_checkpoints = 0
    failures = 0

    async with app.run_test(size=GATE_SIZE) as pilot:
        stop = asyncio.Event()

        async def sample() -> None:
            nonlocal checkpoints, ownership_checkpoints, failures
            next_mark = RSS_SAMPLE_EVERY
            while not stop.is_set():
                await asyncio.sleep(0.02)
                applied = app.frames_applied
                if applied >= next_mark:
                    rss_series.append((applied, _rss_mb()))
                    # A memory sample is also a natural pause point, so the
                    # content check runs at the same cadence rather than
                    # needing a second schedule.
                    checkpoints += 1
                    # Both halves: the domain kept the conversation, and the
                    # interface is actually showing it.
                    if not content_is_complete(app.state, transcript_view(app.state)):
                        failures += 1
                    # The *line-window* half of the interface check still
                    # only runs at the settled checkpoint, for the reason
                    # explained below — it compares the pane's on-screen
                    # window against a moving projection, which is a race
                    # mid-stream. Mounted-window ownership (U6, Part 1) is
                    # different: it is self-contained against each mounted
                    # block document's own current text, never compared
                    # against an external, moving projection, so it is a
                    # true invariant at every instant and is asserted at
                    # every checkpoint (R5/R14's progressiveness) rather
                    # than deferred to settled. RA3's banner accounting is
                    # likewise a same-pass invariant (a banner is mounted
                    # alongside its content, never after), so it is checked
                    # here too — but only at an instant when no apply() is
                    # mid-flight. Textual's Markdown.update sets a
                    # document's source before its children finish
                    # remounting, so inside an apply the "blocks cover the
                    # text" claim is legitimately, transiently false; this
                    # sampler landed in that window once or twice per
                    # full-scale stress run and reported scheduling as
                    # content loss. Yield until the pane is quiescent
                    # (applies are bounded-latency, KTD1(d)); if it never
                    # goes quiescent within the bound, skip this ownership
                    # sample rather than fail it — the settled checkpoint
                    # below asserts the marker actually clears, AND the
                    # ownership samples that actually ran are counted
                    # separately (``enough_ownership_checkpoints``), so a
                    # stream busy enough to defeat quiescence at every
                    # checkpoint fails the floor instead of passing with
                    # every progressive proof silently skipped (CR6).
                    for _ in range(10_000):
                        if not app.transcript.apply_in_flight:
                            break
                        await asyncio.sleep(0)
                    if not app.transcript.apply_in_flight:
                        ownership_checkpoints += 1
                        block_ok, _ = block_documents_are_owned(app.transcript)
                        banner_ok, _ = fallback_banner_accounting(app.transcript)
                        # The expected-documents half runs mid-stream against
                        # the pane's own LAST-APPLIED entries snapshot, never
                        # the live domain state: mid-stream the domain is a
                        # moving projection an unknown number of flushes
                        # ahead, and deriving the expected set from it fails
                        # a correct pane for not having flushed yet. The
                        # snapshot is exactly what the pane just reconciled,
                        # so at a quiescent instant the proof is exact — an
                        # eligible tail wrongly line-rendered during
                        # streaming fails here instead of being repaired
                        # before the settled checkpoint ever looks (CR6).
                        snapshot = app.transcript.last_applied_entries
                        expected_ok = True
                        if snapshot is not None:
                            expected_ok, _ = expected_block_documents_are_mounted(
                                app, entries_view=snapshot
                            )
                        if not (block_ok and banner_ok and expected_ok):
                            failures += 1
                    # The line-window half runs only at the settled
                    # checkpoint, not here. A coalescing renderer is, by
                    # design, an unknown number of flushes behind the
                    # projection at any given instant: it can show fewer lines
                    # than exist (nothing flushed yet) and briefly more than
                    # exist (a completing turn shortened the projection under
                    # a window the pane still holds). Every invariant strong
                    # enough to be worth asserting is therefore a race here,
                    # and asserting one produced a steady one-to-eleven
                    # spurious failures per run against a pane that was
                    # provably correct the moment the stream stopped. What is
                    # checked mid-stream is the domain claim and the
                    # mounted-window/banner claims above; the line-window
                    # claim is checked where it is true.
                    while applied >= next_mark:
                        next_mark += RSS_SAMPLE_EVERY

        sampler = asyncio.create_task(sample())
        try:
            await app.drain(timeout=timeout)
        finally:
            stop.set()
            sampler.cancel()

        # A real pause, then the final checkpoint: the projection has to be
        # complete when the stream stops, not only while it is moving.
        controls.pause()
        await pilot.pause()
        # "Settled" has to mean a flush has actually run, not merely that the
        # stream stopped. The coalescing timer may not fire again after the last
        # frame lands, which left the pane one flush behind the projection and
        # failed the interface check below against a pane that was correct the
        # moment it was allowed to catch up.
        await app.render_snapshot()
        await pilot.pause()
        checkpoints += 1
        final_view = transcript_view(app.state)
        if app.transcript.apply_in_flight:
            # The drain finished and two pauses ran; an apply still marked
            # in-flight here is a stuck marker, and a stuck marker would
            # have silently excused every mid-stream ownership sample above.
            failures += 1
        elif not content_is_complete(app.state, final_view):
            failures += 1
        elif not interface_shows_everything(app, settled=True):
            failures += 1
        rss_series.append((app.frames_applied, _rss_mb()))

        raw = app.measurements()
        refusals = tuple(outcome.name for outcome in controls.refusals)
        final_state = app.state
        await app.shutdown_sources()

    growth = rss_series[-1][1] - rss_series[0][1]
    measurement = GateMeasurement(
        corpus=identity,
        frames_applied=int(raw["frames_applied"]),
        render_ticks=int(raw["render_ticks"]),
        elapsed_seconds=float(raw["elapsed_seconds"]),
        render_ticks_per_second=float(raw["render_ticks_per_second"]),
        peak_mounted_widgets=int(raw["peak_mounted_widgets"]),
        peak_descendant_widgets=int(raw["peak_descendant_widgets"]),
        condensed_lines=int(raw["condensed_lines"]),
        rss_series_mb=rss_series,
        rss_growth_mb=growth,
        rss_slope_mb_per_1k_frames=_fit_slope(rss_series),
        content_loss_checkpoints=checkpoints,
        ownership_checkpoints=ownership_checkpoints,
        content_loss_failures=failures,
        transcript_entries=len(final_state.transcript),
        transcript_lines=final_view.total_lines,
    )
    return measurement, final_state, refusals


async def replay_headless(
    records: tuple[FrameRecord, ...],
    *,
    speed: float,
    pause_after: int | None = None,
    sideband: tuple[SidebandAction, ...] = (),
) -> SessionState:
    """Replay through the app at a given speed and return the final domain state.

    Used for AE11's determinism claim. Runs the real app rather than the reducer
    alone, because the claim under test is that *the interface* produces the
    same state at any speed, and a reducer-only comparison would prove something
    weaker. ``sideband`` extends that claim to include U6's scripted actions
    (R12: "sideband included") — see :func:`measure_replay`'s own docstring
    for why the source is armed after the app exists rather than before.
    """
    controls = ReplayControls()
    controls.set_speed(speed)
    source = ReplaySource(records, controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    if sideband:
        source.bind_sideband(sideband, lambda action: _apply_sideband_action(app, action))
    async with app.run_test(size=GATE_SIZE) as pilot:
        if pause_after is not None:
            while app.frames_applied < pause_after and not app.replay_complete.is_set():
                await asyncio.sleep(0.005)
            controls.pause()
            await pilot.pause()
            controls.resume()
        await app.drain()
        state = app.state
        await app.shutdown_sources()
    return state


async def exercise_inert_controls(records: tuple[FrameRecord, ...]) -> tuple[str, ...]:
    """Invoke every mutation control mid-replay and collect the refusals (AE11)."""
    controls = ReplayControls()
    controls.set_speed(1.0)
    source = ReplaySource(records, controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test(size=GATE_SIZE) as pilot:
        await pilot.pause()
        app.action_interrupt()
        app.composer.text = "this must not be sent"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        refusals = tuple(outcome.name for outcome in controls.refusals)
        retained = app.composer.text
        await app.shutdown_sources()
    if retained != "this must not be sent":  # pragma: no cover - guarded by tests too
        raise AssertionError("the composer lost its text when a replay submit was refused")
    return refusals


def build_matrix() -> dict[str, str]:
    """The exercised platform matrix PC7 asks the gate to record."""
    import textual

    return {
        "os": f"{platform.system()} {platform.release()}",
        "machine": platform.machine(),
        "python": platform.python_version(),
        "textual": textual.__version__,
    }


async def run_gate(
    *,
    live_corpus: str | Path | None = None,
    deltas: int = 50_000,
    seed: int = 20260802,
    mount_cap: int = DEFAULT_MOUNT_CAP,
) -> GateResult:
    """Run the whole gate and return every measurement plus the verdict."""
    stress = build_stress_corpus(deltas=deltas, seed=seed)
    stress_identity = stress_corpus_identity(stress)
    stress_measurement, _, _ = await measure_replay(
        stress.records, stress_identity, mount_cap=mount_cap
    )

    # The sustained pass. The unbounded pass above answers "can the reducer
    # keep up" and it answers it emphatically — which is exactly why it cannot
    # answer "does the renderer keep up": the corpus is gone before the render
    # timer has fired a handful of times. This pass replays on the recorded
    # cadence, scaled so the run occupies roughly
    # :data:`CADENCE_WINDOW_SECONDS`, so the coalescing tick runs hundreds of
    # times against a live stream and the render-tick threshold means something.
    cadence_measurement, _, _ = await measure_replay(
        stress.records,
        stress_identity,
        mount_cap=mount_cap,
        speed=_cadence_speed(stress.records),
    )

    # U6, gap 4: the scripted feature corpus — every R1 construct, early
    # termination by cancel/error/typed-disconnect (mid-table included),
    # parser attacks, one representative of every R7 kind group. Run
    # unconditionally (unlike live_corpus): it needs no operator-supplied
    # recording, and gap 4's whole point is that this corpus exists.
    feature = build_feature_corpus()
    feature_identity = feature_corpus_identity(feature)
    feature_measurement, _, _ = await measure_replay(
        feature.records, feature_identity, mount_cap=mount_cap, sideband=feature.sideband
    )

    # U6, gap 1 (AE2 at gate level) + gap 3 (R12): the sideband-bearing
    # corpus replayed at three transport treatments, compared both on full
    # domain-state equality (the same claim determinism_identical makes) and
    # on normalized block structure (R12's additional comparison).
    sideband_fast = await replay_headless(feature.records, speed=64.0, sideband=feature.sideband)
    sideband_paused = await replay_headless(
        feature.records,
        speed=64.0,
        pause_after=len(feature.records) // 2,
        sideband=feature.sideband,
    )
    sideband_unbounded = await replay_headless(
        feature.records, speed=float("inf"), sideband=feature.sideband
    )
    sideband_determinism_identical = sideband_fast == sideband_paused == sideband_unbounded
    sideband_structure_identical = (
        normalized_block_structure(sideband_fast)
        == normalized_block_structure(sideband_paused)
        == normalized_block_structure(sideband_unbounded)
    )

    # U6, gap 2: the KTD1(d) adversarial workloads (growth curves plus the
    # boundary probes), run once at their exact specified sizes.
    workloads = await run_adversarial_workloads()

    live_measurement: GateMeasurement | None = None
    #: None means "not measured", which is not the same as True. This defaulted
    #: to True and was published verbatim, so a run that never replayed a live
    #: corpus still reported determinism_identical: true having compared
    #: nothing at all.
    determinism_identical: bool | None = None
    refusals: tuple[str, ...] = ()

    if live_corpus is not None and not Path(live_corpus).exists():
        # A typo in the path used to degrade the gate from ten checks to seven
        # and still exit 0. Silently measuring less than you claim to is the
        # failure mode this whole module exists to avoid.
        raise GateError(f"live corpus not found: {live_corpus}")

    if live_corpus is not None:
        records = load_frame_records(live_corpus)
        identity = live_corpus_identity(live_corpus, records)
        live_measurement, _, _ = await measure_replay(records, identity, mount_cap=mount_cap)

        # AE11: same corpus, three transport treatments, one final state.
        fast = await replay_headless(records, speed=64.0)
        paused = await replay_headless(records, speed=64.0, pause_after=len(records) // 2)
        unbounded_state = await replay_headless(records, speed=float("inf"))
        determinism_identical = fast == paused == unbounded_state
        refusals = await exercise_inert_controls(records)

    checks: dict[str, dict[str, Any]] = {
        # Frames the app applied against frames the corpus contains. Content
        # loss upstream of the reducer is invisible to a check whose ground
        # truth is the reducer's own output: discarding nine of every ten
        # inbound frames destroyed 90% of the conversation and still reported
        # zero content loss, because the destroyed frames were never in the
        # ground truth to begin with. This is the accounting that notices.
        "frames_accounted_for": {
            "description": "frames applied equals frames in the stress corpus",
            "measured": stress_measurement.frames_applied,
            "threshold": stress_identity.frame_count,
            "comparison": "==",
            "pass": stress_measurement.frames_applied == stress_identity.frame_count,
        },
        # A check that runs zero times reports zero failures. Both sampled
        # checks below ride a 20ms poll that skips boundaries wholesale on a
        # fast machine, so the sample count is itself a pass condition.
        "enough_memory_samples": {
            "description": "memory series has enough points to fit a slope worth publishing",
            "measured": len(stress_measurement.rss_series_mb),
            "threshold": MIN_RSS_SAMPLES,
            "comparison": ">=",
            "pass": len(stress_measurement.rss_series_mb) >= MIN_RSS_SAMPLES,
        },
        "enough_content_checkpoints": {
            "description": "content completeness was actually checked, not merely scheduled",
            "measured": stress_measurement.content_loss_checkpoints,
            "threshold": MIN_CONTENT_CHECKPOINTS,
            "comparison": ">=",
            "pass": stress_measurement.content_loss_checkpoints >= MIN_CONTENT_CHECKPOINTS,
        },
        # The ownership proofs run only at checkpoints where the pane went
        # quiescent inside the bounded wait — a skipped wait is a skipped
        # proof, and a stream busy enough to defeat every wait would satisfy
        # the checkpoint floor above with zero progressive ownership proofs
        # behind it. This floor counts the proofs that actually ran (CR6).
        "enough_ownership_checkpoints": {
            "description": "mid-stream ownership was actually proven at quiescent "
            "checkpoints, not merely scheduled and skipped",
            "measured": stress_measurement.ownership_checkpoints,
            "threshold": MIN_CONTENT_CHECKPOINTS,
            "comparison": ">=",
            "pass": stress_measurement.ownership_checkpoints >= MIN_CONTENT_CHECKPOINTS,
        },
        "mounted_widgets": _check(
            "mounted line widgets at any point of the stress corpus",
            stress_measurement.peak_mounted_widgets,
            MOUNTED_WIDGET_CEILING,
        ),
        "descendant_widgets": _check(
            "KTD1(a)'s second ceiling: mounted descendants (table cells included) "
            "at any point of the stress corpus",
            stress_measurement.peak_descendant_widgets,
            DESCENDANT_WIDGET_CEILING,
        ),
        "rss_growth_mb": _check(
            "resident-set growth across the full stress replay (MB)",
            stress_measurement.rss_growth_mb,
            RSS_GROWTH_CEILING_MB,
        ),
        "mounted_widgets_sustained": _check(
            "mounted line widgets during the sustained-streaming pass",
            cadence_measurement.peak_mounted_widgets,
            MOUNTED_WIDGET_CEILING,
        ),
        "descendant_widgets_sustained": _check(
            "mounted descendants during the sustained-streaming pass",
            cadence_measurement.peak_descendant_widgets,
            DESCENDANT_WIDGET_CEILING,
        ),
        "render_ticks_per_second": _check(
            "coalescing flushes per second at maximum replay speed",
            stress_measurement.render_ticks_per_second,
            RENDER_TICKS_PER_SECOND_CEILING,
        ),
        "render_ticks_per_second_sustained": _check(
            "coalescing flushes per second under sustained streaming",
            cadence_measurement.render_ticks_per_second,
            RENDER_TICKS_PER_SECOND_CEILING,
        ),
        "content_loss": {
            "measured": stress_measurement.content_loss_failures,
            "threshold": 0,
            "comparison": "<=",
            "description": "checkpoints at which a domain transcript entry was missing "
            "from the projection",
            "pass": stress_measurement.content_loss_failures == 0,
        },
        "content_loss_sustained": {
            "measured": cadence_measurement.content_loss_failures,
            "threshold": 0,
            "comparison": "<=",
            "description": "same check taken while the stream is still moving",
            "pass": cadence_measurement.content_loss_failures == 0,
        },
        # U6, gap 4: the feature corpus's own accounting and content-loss
        # checks, the same shape as the stress corpus's own pair above.
        "feature_corpus_frames_accounted_for": {
            "description": "frames applied equals frames in the feature corpus",
            "measured": feature_measurement.frames_applied,
            "threshold": feature_identity.frame_count,
            "comparison": "==",
            "pass": feature_measurement.frames_applied == feature_identity.frame_count,
        },
        "feature_corpus_content_loss": {
            "measured": feature_measurement.content_loss_failures,
            "threshold": 0,
            "comparison": "<=",
            "description": "content-loss and ownership-proof checkpoints against the feature "
            "corpus — includes talaria/replay/workloads.py's discovered append-vs-construction "
            "defect if a sampled checkpoint lands inside the streamed table's growth window",
            "pass": feature_measurement.content_loss_failures == 0,
        },
        # U6, gap 1 (AE2 at gate level): early termination by cancel, error,
        # and typed-disconnect all rendered their received content — proven
        # by the feature corpus's own content-loss check above (its
        # early-termination turns are inside that same corpus) plus this
        # replay-determinism check, which additionally proves the sideband
        # itself is deterministic (R30/AE2's "replay it twice, get the same
        # state", extended to the two non-wire-frame action kinds).
        "sideband_replay_determinism": {
            "measured": sideband_determinism_identical,
            "threshold": True,
            "comparison": "==",
            "description": "64x, 64x-with-pause and unbounded replays of the sideband-bearing "
            "feature corpus end in identical domain state",
            "pass": sideband_determinism_identical,
        },
        # U6, gap 3 (R12): the additional normalized-block-structure
        # comparison, sideband included.
        "sideband_normalized_structure_determinism": {
            "measured": sideband_structure_identical,
            "threshold": True,
            "comparison": "==",
            "description": "ordered block classes, source ranges and semantic content "
            "(runtime identifiers excluded) agree across the same three sideband-bearing "
            "replays (R12)",
            "pass": sideband_structure_identical,
        },
    }
    # U6, gap 2: one latency check per KTD1(d) workload, p99 against the
    # 50ms coalescing-interval ceiling.
    for report in workloads.reports:
        checks[f"workload_latency_{report.label}"] = {
            "measured": round(report.p99_ms, 3),
            "threshold": workloads.latency_ceiling_ms,
            "comparison": "<=",
            "description": f"steady-state p99 TranscriptPane.apply latency for the "
            f"{report.label} workload (ms): the first {WARMUP_BOUNDARIES} boundaries are "
            "excluded as warm-up, and when the workload demoted, the entire block phase "
            "through the demotion boundary is excluded too (RA4/RA5) — the block-phase "
            "peak and the demotion cost are reported verbatim in the report, recorded "
            "rather than enforced",
            "pass": report.p99_ms <= workloads.latency_ceiling_ms,
        }
    if live_measurement is not None:
        checks["live_corpus_content_loss"] = {
            "measured": live_measurement.content_loss_failures,
            "threshold": 0,
            "comparison": "<=",
            "description": "same check against the recorded session",
            "pass": live_measurement.content_loss_failures == 0,
        }
        checks["replay_determinism"] = {
            "measured": determinism_identical,
            "threshold": True,
            "comparison": "==",
            # Says what the code actually replays. The previous wording claimed
            # "1x-with-pause", and 1x is never replayed -- the pause runs at 64x
            # like the baseline, so this exercises two distinct speeds against
            # AE11's "at any speed", not three. Overstating the treatments in a
            # published check description is the same class of defect as
            # overstating a threshold.
            "description": "64x, 64x-with-pause and unbounded replays end in identical "
            "domain state (AE11); MIN_SPEED is not replayed -- see QUEUED.md",
            "pass": determinism_identical,
        }
        checks["inert_mutation_controls"] = {
            "measured": sorted(set(refusals)),
            "threshold": ["interrupt", "submit"],
            "comparison": "==",
            "description": "mutation controls invoked mid-replay refused and said so",
            "pass": sorted(set(refusals)) == ["interrupt", "submit"],
        }

    return GateResult(
        stress=stress_measurement,
        cadence=cadence_measurement,
        live=live_measurement,
        determinism_identical=determinism_identical,
        inert_controls_refused=refusals,
        matrix=build_matrix(),
        checks=checks,
        feature=feature_measurement,
        sideband_determinism_identical=sideband_determinism_identical,
        sideband_structure_identical=sideband_structure_identical,
        workloads=workloads,
    )


def _cadence_speed(records: tuple[FrameRecord, ...]) -> float:
    """The multiplier that makes a corpus take about one measurement window.

    Derived from the corpus's own recorded duration rather than fixed, so the
    sustained pass lasts the same wall-clock time whether the corpus is a
    four-turn recording or a 50,000-delta generated stream. Clamped into the
    controls' finite range, so a corpus far too short to fill the window simply
    replays as slowly as the controls allow rather than pretending.
    """
    if len(records) < 2:
        return 1.0
    recorded = records[-1].at - records[0].at
    if recorded <= 0:
        return 1.0
    return max(MIN_SPEED, min(MAX_SPEED, recorded / CADENCE_WINDOW_SECONDS))


def _check(description: str, measured: float, threshold: float) -> dict[str, Any]:
    return {
        "measured": round(measured, 3),
        "threshold": threshold,
        "comparison": "<=",
        "description": description,
        "pass": measured <= threshold,
    }
