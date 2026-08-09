"""The transcript pane: a hybrid of block markdown documents and line widgets
(U4, KTD1/KTD2/KTD3).

**The unit changed from "line" to "entry", for committed assistant and
reasoning entries only.** Every other kind — user, tool, subagent, system,
prompt, error — still mounts one :class:`TranscriptLine` per line, exactly as
v0.1 did. A committed assistant or reasoning entry instead mounts one
:class:`~talaria.ui.blocks.EntryMarkdown` document, built by
``talaria/ui/blocks.py`` (U3), so headings, fences, lists, quotes, and tables
render as themselves rather than as flat coloured text. The in-flight
assistant and reasoning streams each get their own live document too — two,
not one, because the domain holds both buffers at once and R18 forbids either
stealing the other's progressive rendering.

**Two ceilings, not one (KTD1(a)).** The folded window — every committed
entry except the newest and the two live tails — stays under a *descendant*
budget, not a widget-count budget, because one table can mount hundreds of
per-cell widgets from a single top-level block. The newest entry and each
tail instead carry a per-entry two-condition trigger: an entry whose
construct-aware descendant estimate exceeds 1,200, or whose width-aware
wrapped-row estimate exceeds :data:`DEFAULT_MOUNT_CAP`, falls back to
non-wrapping line rendering with one banner widget — never fully flattened,
never silently dropped, painted rows pinned to exactly the projected line
count plus one.

**Condensation folds whole units, with one exception per unit class.** A
block-rendered entry is atomic: a fold boundary landing inside it rounds up
and evicts the whole entry. A fallen-back or ordinary line-rendered entry is
still line-content, foldable a row at a time — except a fallen-back entry's
banner is charged to the same budget as its content rows and is never left
standing alone: a cut that would retain zero content rows takes the banner
with it (rounds forward); a cut that retains at least one content row keeps
exactly one banner beside it.

**Why entry identity matters here and did not in v0.1.** Once a committed
entry equals a mounted document rather than a slice of a flat line array, the
mapping from "this projection line moved" to "this widget needs a poke" is no
longer positional — it is by entry id, published by
:func:`~talaria.domain.projection.entry_scoped_view` (KTD6). Every method
below that walks entries reads that id, never a line offset, and the one
place a widget survives unchanged across a state transition — a stream
handing its document to the entry it just became — is keyed on it too.
"""

from __future__ import annotations

import math
from collections import deque
from dataclasses import dataclass, field
from typing import Final, Literal

from rich.cells import cell_len
from rich.text import Text
from textual.containers import VerticalScroll
from textual.widget import Widget
from textual.widgets import Static

from talaria.domain.models import TranscriptKind
from talaria.domain.projection import (
    EntryScopedView,
    ProvisionalTail,
    TranscriptEntryRecord,
    TranscriptView,
)
from talaria.ui.blocks import EntryMarkdown, parser_factory
from talaria.ui.literal import defang, literal_text
from talaria.ui.markdown import inline_markdown

#: KTD1(a)'s tier-two line cap and the wrapped-rows trigger threshold. The
#: name and the number are unchanged from v0.1 (ADR-0005 decision 3, amended
#: by ADR-0006 rather than replaced) — this is still the line-widget cap for
#: line-rendered surfaces, and it doubles as the per-entry wrapped-row
#: trigger. The folded-window *descendant* ceiling the gate enforces is a
#: separate number (600), read off the live tree, not stated here.
DEFAULT_MOUNT_CAP: Final[int] = 500

#: KTD1(a) tier two: an entry (or live tail) whose construct-aware descendant
#: estimate exceeds this falls back to line rendering. Calibrated against
#: Textual 8.2.8 live probes (see :func:`descendant_estimate`), not guessed.
DESCENDANT_ESTIMATE_TRIGGER: Final[int] = 1_200

CONDENSED_TEMPLATE: Final[str] = "── {count} earlier lines condensed (still readable by the agent)"

#: RA3's banner: names the clip and the fallback cause, never claims the
#: clipped cells are reachable on screen (the full line is still in the
#: terminal-read buffer, which the banner also says).
FALLBACK_BANNER_TEMPLATE: Final[str] = (
    "── entry too large to render as markdown ({lines} lines clipped at the "
    "viewport edge, full content still readable by the agent) ──"
)

#: The entry kinds that attempt block rendering. Moved here from
#: ``talaria/ui/markdown.py`` (KTD8): this is the set U4 mounts as
#: :class:`~talaria.ui.blocks.EntryMarkdown` documents, and it is the *only*
#: remaining live caller of the presentational question "is this agent
#: prose". ``talaria/ui/markdown.py`` keeps ``inline_markdown`` in the tree as
#: the KTD8 fallback's styling half, but nothing on this pane's live path
#: calls it any more — a kind that stays out of this set is line-rendered
#: with :func:`~talaria.ui.literal.literal_text`, plain, same as ``tool`` or
#: ``user``.
MARKDOWN_KINDS: Final[frozenset[TranscriptKind]] = frozenset({"assistant", "reasoning"})

#: The two independently keyed live tails (R18). A ``list`` would let a typo
#: silently create a third; the tuple is what :meth:`TranscriptPane.apply`
#: iterates, in this order — reasoning before assistant, because within one
#: turn an agent's reasoning precedes the reply it explains.
_TAIL_KINDS: Final[tuple[TranscriptKind, ...]] = ("reasoning", "assistant")

#: The presentation weld a line-rendered surface still carries — a verbatim
#: copy of ``talaria/domain/projection.py``'s private ``_ENTRY_PREFIX``,
#: duplicated rather than imported because that name is private to a module
#: outside U4's file list. KTD6 makes the weld decoration that applies only
#: to line-rendered surfaces: :func:`~talaria.domain.projection.entry_scoped_view`
#: hands this pane an *unwelded* ``raw_body`` for every entry (a block
#: document must never receive a welded first line — ``· `` in front of a
#: reasoning entry's own heading is invalid markdown, R18's exact defect),
#: but ``TranscriptView.lines`` — what :attr:`TranscriptPane.rendered_lines`
#: has to keep matching — still carries the weld for every kind, block or
#: not, via :func:`~talaria.domain.projection.transcript_view`'s
#: ``_entry_lines``. A unit that falls back to line rendering therefore
#: welds this prefix onto its own first line before building widgets, the
#: same way ``_entry_lines`` welds it before splitting. **If this table ever
#: drifts from projection.py's, the drift is silent** — no test in this
#: unit's list can see across that module boundary — which is exactly the
#: caveat this comment exists to flag for whoever next edits either table.
_ENTRY_PREFIX: Final[dict[str, str]] = {
    "user": "› ",
    "assistant": "",
    "reasoning": "· ",
    "tool": "",
    "subagent": "  ",
    "system": "— ",
    "prompt": "? ",
    "prompt-expired": "? ",
    "cancelled": "",
    "error": "! ",
    "protocol-error": "! ",
    "unknown-event": "! ",
}


# ── construct-aware descendant estimate (KTD1(a)) ──────────────────────────

#: Per-item overhead for a list item, and per-container overhead for the
#: ``bullet_list``/``ordered_list`` wrapper — probed live against Textual
#: 8.2.8: three flat items mount 13 descendants (1 container + 3 × 4), and
#: the same three items with one nested one level down mount 14 (2
#: containers + 3 × 4). Both numbers are exact, not headroom.
_LIST_ITEM_OVERHEAD: Final[int] = 4
_LIST_CONTAINER_OVERHEAD: Final[int] = 1

#: A blockquote or a fence, whatever its length, probed at exactly 2
#: descendants each (a 10,000-line open fence included — the container plus
#: its one content widget). A paragraph or heading is exactly 1.
_TWO_WIDGET_BLOCK_TYPES: Final[frozenset[str]] = frozenset({"blockquote_open", "fence", "hr"})
_ONE_WIDGET_BLOCK_TYPES: Final[frozenset[str]] = frozenset(
    {"paragraph_open", "heading_open", "code_block", "html_block"}
)

#: Table overhead beyond its cells — probed exactly: a 3-line, 601-column
#: table (601 header cells + 601 body cells) mounts 1,204 descendants, i.e.
#: 1,202 cells + 2. A 599-column table plus one trailing paragraph mounts
#: 1,201 — (599 + 599) + 2 (table) + 1 (paragraph) = 1,201, exactly, which
#: is also the pinned exact-boundary regression: the estimate must exceed
#: 1,200 to fire the trigger, and 1,201 does.
_TABLE_OVERHEAD: Final[int] = 2


def descendant_estimate(markdown_text: str) -> int:
    """The construct-aware descendant count KTD1(a)'s trigger reads.

    Parses once with the same :func:`~talaria.ui.blocks.parser_factory` an
    entry document itself will parse with (KTD4 isolation — no shared parser
    state), then walks the flat token stream counting exactly the widgets
    :class:`~talaria.ui.blocks.EntryMarkdown` will mount: one per ordinary
    top-level block, two for a blockquote or fence, per-item and
    per-container overhead for lists, and cells-plus-overhead for a table —
    every constant probed live against the installed Textual 8.2.8 rather
    than assumed (see the module constants above). Returns 0 for a
    zero-block source; callers that need "is this entry too large" should
    check :func:`is_zero_block` first, since 0 also means "nothing to
    fall back from".
    """
    tokens = parser_factory().parse(markdown_text)
    total = 0
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if token.type == "table_open":
            depth = 1
            index += 1
            while index < len(tokens) and depth:
                ttype = tokens[index].type
                if ttype == "table_open":
                    depth += 1
                elif ttype == "table_close":
                    depth -= 1
                elif ttype in ("th_open", "td_open"):
                    total += 1
                index += 1
            total += _TABLE_OVERHEAD
            continue
        if token.type in ("bullet_list_open", "ordered_list_open"):
            total += _LIST_CONTAINER_OVERHEAD
        elif token.type == "list_item_open":
            total += _LIST_ITEM_OVERHEAD
        elif token.type in _TWO_WIDGET_BLOCK_TYPES:
            total += 2
        elif token.type in _ONE_WIDGET_BLOCK_TYPES:
            total += 1
        index += 1
    return total


def is_zero_block(markdown_text: str) -> bool:
    """True for empty, whitespace-only, or newline-only sources (KTD2).

    A zero-block source parses to no tokens at all and, mounted through
    :class:`~talaria.ui.blocks.EntryMarkdown`, would be a height-zero
    document — probed live — which would silently drop the blank rows a
    line widget shows today. Checked by parsing rather than by
    ``not text.strip()`` so the definition matches the parser's, not a
    guess about it: both agree in every case that has been probed, and this
    is the one that would keep agreeing if that ever stopped being true.
    """
    return len(parser_factory().parse(markdown_text)) == 0


def wrapped_row_estimate(text: str, content_width: int) -> int:
    """Width-aware wrapped-row count, in **display cells**, not characters.

    ``sum(max(1, ceil(cell_len(line) / content_width)) for line in
    text.split("\\n"))`` — :func:`rich.cells.cell_len` is Textual's own
    terminal display-cell width measure, the same one the widget's own
    layout uses, so a double-width character counts as two cells here the
    same as it will on screen. A character count would undercount a
    double-width line by up to half (a probed 37,000-character CJK line
    paints 949 rows; a character count estimates 475), which is exactly
    the gap this function exists to close.
    """
    if content_width <= 0:
        return sum(1 for _ in text.split("\n"))
    total = 0
    for line in text.split("\n"):
        total += max(1, math.ceil(cell_len(line) / content_width))
    return total


def trips_fallback_trigger(markdown_text: str, *, content_width: int) -> bool:
    """KTD1(a)'s two-condition trigger: descendants over 1,200, or wrapped
    rows over :data:`DEFAULT_MOUNT_CAP` — either alone is sufficient.
    """
    return (
        descendant_estimate(markdown_text) > DESCENDANT_ESTIMATE_TRIGGER
        or wrapped_row_estimate(markdown_text, content_width) > DEFAULT_MOUNT_CAP
    )


# ── line widgets ─────────────────────────────────────────────────────────


class TranscriptLine(Static):
    """One projected line — unchanged from v0.1 for ordinary line-rendered
    kinds; also used, with ``no_wrap=True``, as the fallen-back entry's
    per-line content widget (KTD1(a)).
    """

    def __init__(
        self, source: str, *, kind: TranscriptKind | None = None, no_wrap: bool = False
    ) -> None:
        if no_wrap:
            renderable: Text = Text(defang(source), no_wrap=True, end="")
        elif kind in MARKDOWN_KINDS:
            renderable = inline_markdown(source)
        else:
            renderable = literal_text(source)
        css_class = "transcript--nowrap" if no_wrap else None
        super().__init__(renderable, markup=False, classes=css_class)
        #: The projection line, verbatim — not the text that ends up on screen.
        self.source = source


# ── mounted-unit bookkeeping ────────────────────────────────────────────

UnitKind = Literal["block", "line"]


@dataclass
class _MountedUnit:
    """One committed entry's or one tail's mounted representation.

    ``line_count`` is the number of *projected* content lines this unit
    covers — for a block-rendered unit that is its whole projected span; for
    a line/fallback unit it is ``len(lines)``, which can shrink from the
    left as condensation folds it. ``is_fallback`` marks the KTD1(a)
    banner-charge rule: only fallback units may carry a banner, and the
    banner is chrome, charged to the fold arithmetic but never counted as a
    projected content line (KTD2's "banner rows... never enter the
    condensed_count + lines-accounted-for == total content identity").
    """

    kind: UnitKind
    entry_id: int | TranscriptKind
    block: EntryMarkdown | None = None
    lines: list[TranscriptLine] = field(default_factory=list)
    banner: Static | None = None
    is_fallback: bool = False
    #: The full text this unit was last built from — used to compute an
    #: append suffix for a same-generation tail update, and to detect a
    #: same-text no-op.
    applied_text: str = ""

    @property
    def line_count(self) -> int:
        return len(self.lines)

    @property
    def accounted_rows(self) -> int:
        """Content rows plus the one banner row, if any (KTD1(a) charge)."""
        return self.line_count + (1 if self.banner is not None else 0)

    def widgets(self) -> list[Widget]:
        if self.kind == "block":
            assert self.block is not None
            return [self.block]
        out: list[Widget] = list(self.lines)
        if self.banner is not None:
            out.append(self.banner)
        return out


def _fallback_banner(line_count: int) -> Static:
    text = literal_text(FALLBACK_BANNER_TEMPLATE.format(lines=line_count))
    return Static(text, markup=False, classes="transcript--fallback-banner")


class TranscriptPane(VerticalScroll):
    """Scrollable, bounded transcript: block documents plus line widgets."""

    DEFAULT_CSS = """
    TranscriptPane {
        height: 1fr;
        scrollbar-size-vertical: 1;
    }
    TranscriptPane > Static {
        width: 100%;
    }
    TranscriptPane > .transcript--condensed {
        color: $text-muted;
    }
    TranscriptPane > .transcript--fallback-banner {
        color: $text-warning;
    }
    TranscriptPane .transcript--nowrap {
        height: 1;
    }
    """

    def __init__(self, *, mount_cap: int = DEFAULT_MOUNT_CAP, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.mount_cap = max(1, mount_cap)
        self.follow = True
        self._lines: tuple[str, ...] = ()
        #: Real projected content lines folded away — a pure line-index
        #: concept, never inflated by a fallback banner (KTD2).
        self._top = 0
        self._condensed: Static | None = None
        self.peak_mounted = 0
        #: Committed entries currently mounted, in commit order, keyed by
        #: entry id (KTD6) — never by line position.
        self._entries: dict[int, _MountedUnit] = {}
        self._entry_order: deque[int] = deque()
        #: The two live tails, keyed by kind (R18). ``None`` means empty —
        #: nothing mounted, matching :attr:`ProvisionalTail.is_empty`.
        self._tails: dict[TranscriptKind, _MountedUnit | None] = {
            "assistant": None,
            "reasoning": None,
        }
        self._tail_generation: dict[TranscriptKind, int] = {"assistant": 0, "reasoning": 0}
        self._pending_removed_height = 0

    # ── measurement surface ──────────────────────────────────────────────

    @property
    def mounted_count(self) -> int:
        """Widgets actually in the tree — read straight from Textual, never
        from this class's own bookkeeping (unchanged rationale from v0.1)."""
        return len(self.children)

    @property
    def descendant_count(self) -> int:
        """Every mounted widget, tables' per-cell children included (KTD1(a)
        tier one, read the way the gate reads it: ``walk_children``, not a
        count this class computed about itself)."""
        return len(list(self.walk_children()))

    @property
    def condensed_count(self) -> int:
        return self._top

    @property
    def rendered_lines(self) -> tuple[str, ...]:
        """Content reconstruction (U4): line widgets contribute their line;
        block entries contribute their projected source lines via the
        entry's own raw body, split on newlines — never the banner, which
        is chrome and stays out of this identity (KTD2). This is what keeps
        ``pane.rendered_lines == view.lines[pane.condensed_count:]`` true
        under mixed mounting, exactly as it was true under pure line
        mounting.
        """
        out: list[str] = []
        for entry_id in self._entry_order:
            unit = self._entries[entry_id]
            if unit.kind == "block":
                out.extend(unit.applied_text.split("\n"))
            else:
                out.extend(widget.source for widget in unit.lines)
        for kind in _TAIL_KINDS:
            tail_unit = self._tails[kind]
            if tail_unit is None:
                continue
            if tail_unit.kind == "block":
                out.extend(tail_unit.applied_text.split("\n"))
            else:
                out.extend(widget.source for widget in tail_unit.lines)
        return tuple(out)

    @property
    def drawn_lines(self) -> tuple[str, ...]:
        """The characters the terminal actually paints, per projected line.

        Exact for every line-rendered unit (the drawn ``Static.content``,
        same as v0.1). A block-rendered unit's per-line drawn text is not
        separably knowable from the mounted document — Textual renders it
        as blocks, not lines — so a block unit's contribution here falls
        back to its projected source lines, same as :attr:`rendered_lines`.
        Consumers that need the literal painted characters of a block entry
        should read the document's own blocks instead of this property.
        """
        out: list[str] = []
        for entry_id in self._entry_order:
            unit = self._entries[entry_id]
            if unit.kind == "block":
                out.extend(unit.applied_text.split("\n"))
            else:
                out.extend(str(widget.content) for widget in unit.lines)
        for kind in _TAIL_KINDS:
            tail_unit = self._tails[kind]
            if tail_unit is None:
                continue
            if tail_unit.kind == "block":
                out.extend(tail_unit.applied_text.split("\n"))
            else:
                out.extend(str(widget.content) for widget in tail_unit.lines)
        return tuple(out)

    # ── content width (RA2 / KTD1(a)) ───────────────────────────────────

    @property
    def _content_width(self) -> int:
        """The width the wrapped-row estimate is computed against.

        Four columns narrower than the pane's own width, not equal to it: a
        mounted ``Markdown`` document pads two columns on each side (RA2's
        grounding probed an 80-column pane down to 76 columns of actual
        content box). Estimating at the wider, unpadded pane width would
        *undercount* wrapped rows relative to what the document actually
        wraps at — the trigger needs to fire no later than the real overflow
        does, so this errs narrow rather than trusting the pane's raw size.
        """
        width = self.size.width - 4
        return width if width > 0 else 76

    # ── update ───────────────────────────────────────────────────────────

    async def apply(self, view: TranscriptView, entries: EntryScopedView) -> None:
        """Reconcile the mounted widgets with a new projection snapshot.

        Two inputs, not one, because a block-rendering pane needs entry
        identity and raw (unwelded) bodies that the flattened line buffer
        does not carry (KTD6) — ``view`` is still read for
        :attr:`condensed_count`'s line arithmetic and for the kinds that stay
        line-rendered outside the entry-scoped surface entirely (none do
        today; every mounted kind is covered by ``entries``, but ``view`` is
        the source of truth for total line count either way).
        """
        await self._reset_if_history_changed(entries.entries)
        await self._reconcile_committed(entries.entries)
        await self._reconcile_tail("reasoning", entries.reasoning_tail)
        await self._reconcile_tail("assistant", entries.assistant_tail)
        await self._condense(entries.entries)
        await self._render_condensed()

        self._lines = view.lines
        self.peak_mounted = max(self.peak_mounted, self.mounted_count)
        self._restore_anchor()

    # ── hard reset (a focus switch, not a continuation) ─────────────────

    async def _reset_if_history_changed(
        self, records: tuple[TranscriptEntryRecord, ...]
    ) -> None:
        """Clear every mounted widget when ``records`` is not a continuation
        of what is already mounted (a session switch, not new content).

        Entries are append-only *within one session's transcript* (KTD6),
        but ``entries.entries`` is not scoped to "this pane's session" — it
        is scoped to whatever session the domain currently has focused, and
        switching focus swaps in an entirely different, non-overlapping
        entry-id history (a resumed session's own entries, most often
        starting its own numbering back near zero). v0.1's line-indexed
        pane never needed a check like this: comparing flat line arrays by
        stable prefix, a full swap is simply "the first line differs",
        which its ordinary divergence handling already evicted everything
        for. This pane's entry-id keying has no such fallback by
        construction — every entry not yet in ``self._entries`` reads as
        "new content to mount", which is exactly wrong for "content from a
        session this pane is no longer showing." Found live: switching
        sessions left the outgoing session's transcript lines (an
        outstanding approval's summary, most visibly) mounted and on
        screen after the switch, because nothing had ever told this pane
        the old entries no longer belonged.

        The check: any entry id currently mounted that is absent from the
        new ``records`` means the histories are not the same lineage —
        entries are never deleted from a live transcript, so an id going
        missing means the underlying transcript identity changed, not that
        an entry was individually evicted (folding removes an id from
        ``self._entries`` too, but never *below* :attr:`_top`, and this
        check only fires while the pane still holds mounted entries
        pre-dating the new set's earliest coverage).
        """
        if not self._entries:
            return
        current_ids = {record.entry_id for record in records}
        if all(entry_id in current_ids for entry_id in self._entries):
            return
        for entry_id in list(self._entry_order):
            unit = self._entries.pop(entry_id, None)
            if unit is not None:
                for widget in unit.widgets():
                    await self._safe_remove(widget)
        self._entry_order.clear()
        for kind in _TAIL_KINDS:
            unit = self._tails[kind]
            if unit is not None:
                for widget in unit.widgets():
                    await self._safe_remove(widget)
                self._tails[kind] = None
        self._top = 0

    # ── committed entries ────────────────────────────────────────────────

    async def _reconcile_committed(self, records: tuple[TranscriptEntryRecord, ...]) -> None:
        """Mount every entry not yet mounted — except one already folded away.

        Entries are append-only and immutable in the domain (KTD6): nothing
        ever deletes ``entries.entries``, so every record this pane has ever
        condensed keeps reappearing here on every later call, forever. The
        pane's own eviction (:meth:`_condense`) is a *rendering* fact, not a
        domain one, and unlike v0.1's line-indexed pane — where "below
        ``self._top``" was a single integer comparison against the flat line
        array — there was no positive check here at all: an entry id no
        longer in ``self._entries`` looked exactly like an entry that had
        never been mounted, so this method remounted it, appended it to the
        *end* of ``self._entry_order`` regardless of its real commit
        position, and both corrupted commit ordering (a low-numbered entry
        could resurface after high-numbered ones) and double-charged its
        lines — once when it was first folded, again when it came back and
        was condensed a second time. Found live: a 40-turn corpus left
        ``rendered_lines`` and ``condensed_count`` summing to more than the
        projection's total line count. v0.1's own docstring already named
        the rule this replaces: "Nothing un-condenses it" — carried forward
        here by comparing the entry's line span against :attr:`_top` rather
        than checking mounted-widget presence alone.
        """
        # Built up across every new entry and mounted in ONE call at the end
        # (never one `mount_all` per entry). A batch of 60 individual mounts
        # against a live pane left `scroll_end()` observing a scroll offset
        # of 0 instead of the bottom — found live, against a real gateway
        # fixture, not a synthetic one: Textual settles the scrollable
        # region's height once per mount pass, and sixty separate passes
        # each invalidate and re-settle it, racing the one `scroll_end()`
        # call `_restore_anchor` makes at the very end of `apply()` against
        # a virtual size that had not caught up yet. One batch mounts once,
        # settles once, and `scroll_end()` sees the true final height —
        # exactly v0.1's own shape ("mount exactly the window's missing
        # suffix" in one `mount_all` call), which this restores.
        pending: list[Widget] = []
        deferred_writes: list[tuple[EntryMarkdown | None, str]] = []
        for record in records:
            if record.entry_id in self._entries:
                continue
            start, count = record.line_span
            if start + count <= self._top:
                continue
            self._prepare_committed_entry(record, pending, deferred_writes)
        await self._mount_widgets(pending, before_tails=True)
        # R16 clause 2: a terminal path stops and awaits pending widget
        # writes -- these are queued rather than awaited inline above only
        # to keep _prepare_committed_entry synchronous for batching, and
        # `apply()` does not return until every one of them has actually
        # landed.
        for widget, text in deferred_writes:
            await self._safe_write(widget, "update", text)

    def _prepare_committed_entry(
        self,
        record: TranscriptEntryRecord,
        pending: list[Widget],
        deferred_writes: list[tuple[EntryMarkdown | None, str]],
    ) -> None:
        """Build (but do not mount) one entry's unit, queuing its widgets.

        R18/KTD2: hand a live tail's document straight to the entry it just
        became, keyed by kind — no remove, no rebuild, and nothing queued
        for mounting since the widget is already on screen. Only sound when
        the tail's own text is a prefix of (or equal to) the entry's final
        body; a mismatch (the tail was reset without ever reaching this
        text) falls through to building fresh.
        """
        tail = self._tails.get(record.kind)
        if (
            tail is not None
            and tail.kind == "block"
            and record.raw_body.startswith(tail.applied_text)
        ):
            if record.raw_body != tail.applied_text:
                # Queued, not awaited here: _prepare_committed_entry is
                # synchronous (the whole point of the batching fix is
                # queuing every new widget before the one
                # `await self.mount_all(...)` in the caller), but R16
                # clause 2 still requires this write to have actually
                # landed before `apply()` returns — the caller awaits every
                # entry in `deferred_writes` right after the batch mount.
                deferred_writes.append((tail.block, record.raw_body))
                tail.applied_text = record.raw_body
            unit = tail
            self._tails[record.kind] = None
        elif tail is not None and tail.kind == "line" and record.raw_body == tail.applied_text:
            unit = tail
            self._tails[record.kind] = None
        else:
            unit = self._prepare_unit(record.entry_id, record.kind, record.raw_body, pending)
        unit.entry_id = record.entry_id
        self._entries[record.entry_id] = unit
        self._entry_order.append(record.entry_id)

    def _prepare_unit(
        self,
        entry_id: int | TranscriptKind,
        kind: TranscriptKind,
        text: str,
        pending: list[Widget],
    ) -> _MountedUnit:
        """Build one fresh unit — block document, fallen-back line group, or
        ordinary line group, whichever KTD2's rules select — queuing its
        widgets in ``pending`` rather than mounting them immediately.
        """
        width = self._content_width
        eligible = kind in MARKDOWN_KINDS
        if eligible and not is_zero_block(text) and not trips_fallback_trigger(
            text, content_width=width
        ):
            widget = EntryMarkdown(text)
            pending.append(widget)
            return _MountedUnit(kind="block", entry_id=entry_id, block=widget, applied_text=text)

        # Line rendering: either an ordinary non-markdown kind, a zero-block
        # markdown source, or one that tripped the fallback trigger. Welded
        # exactly as transcript_view()'s _entry_lines welds it -- the prefix
        # goes on the whole body before splitting, so it lands on the first
        # line only. This is what keeps rendered_lines (read off each
        # widget's own .source) matching TranscriptView.lines for a
        # line-rendered surface (KTD6: the weld is decoration that belongs
        # to line rendering, never to a parsed block document, so
        # applied_text below -- used for tail-generation bookkeeping against
        # the domain's unwelded raw_text -- stays unwelded).
        welded_body = f"{_ENTRY_PREFIX.get(kind, '')}{text}"
        is_fallback = eligible and not is_zero_block(text) and trips_fallback_trigger(
            text, content_width=width
        )
        source_lines = welded_body.split("\n") if welded_body else [""]
        widgets = [
            TranscriptLine(line, kind=kind, no_wrap=is_fallback) for line in source_lines
        ]
        banner = _fallback_banner(len(widgets)) if is_fallback else None
        pending.extend(widgets)
        if banner is not None:
            pending.append(banner)
        return _MountedUnit(
            kind="line",
            entry_id=entry_id,
            lines=widgets,
            banner=banner,
            is_fallback=is_fallback,
            applied_text=text,
        )

    async def _build_unit(
        self,
        entry_id: int | TranscriptKind,
        kind: TranscriptKind,
        text: str,
        *,
        mount_before_tails: bool,
    ) -> _MountedUnit:
        """Build and mount one fresh unit immediately — the tail path, where
        exactly one unit is built per call and there is nothing to batch.
        """
        pending: list[Widget] = []
        unit = self._prepare_unit(entry_id, kind, text, pending)
        await self._mount_widgets(pending, before_tails=mount_before_tails)
        return unit

    async def _mount_widgets(self, widgets: list[Widget], *, before_tails: bool) -> None:
        if not widgets:
            return
        if before_tails:
            anchor = self._first_tail_widget()
            if anchor is not None:
                await self.mount_all(widgets, before=anchor)
                return
        await self.mount_all(widgets)

    def _first_tail_widget(self) -> Widget | None:
        for kind in _TAIL_KINDS:
            unit = self._tails[kind]
            if unit is not None:
                widgets = unit.widgets()
                if widgets:
                    return widgets[0]
        return None

    # ── live tails (KTD3, R18) ──────────────────────────────────────────

    async def _reconcile_tail(self, kind: TranscriptKind, tail: ProvisionalTail) -> None:
        unit = self._tails[kind]

        if tail.is_empty:
            if unit is not None:
                for widget in unit.widgets():
                    await self._safe_remove(widget)
                self._tails[kind] = None
            self._tail_generation[kind] = tail.generation
            return

        if unit is None:
            new_unit = await self._build_unit(kind, kind, tail.raw_text, mount_before_tails=False)
            self._tails[kind] = new_unit
            self._tail_generation[kind] = tail.generation
            return

        same_generation = tail.generation == self._tail_generation[kind]
        self._tail_generation[kind] = tail.generation

        if unit.kind == "block":
            if same_generation and tail.raw_text.startswith(unit.applied_text):
                suffix = tail.raw_text[len(unit.applied_text) :]
                if suffix:
                    await self._safe_write(unit.block, "append", suffix)
            else:
                await self._safe_write(unit.block, "update", tail.raw_text)
            unit.applied_text = tail.raw_text
            # Re-check the trigger on the grown text; a tail that just
            # crossed the threshold falls back once and stays there for the
            # rest of the stream (ends the growing-reparse work, KTD1(a)).
            if trips_fallback_trigger(tail.raw_text, content_width=self._content_width):
                for widget in unit.widgets():
                    await self._safe_remove(widget)
                self._tails[kind] = await self._build_unit(
                    kind, kind, tail.raw_text, mount_before_tails=False
                )
            return

        # Currently line-rendered (fallback or zero-block). A generation
        # change or a trigger re-check needs a rebuild; a same-generation
        # append that stays under the trigger just grows the line list.
        if not same_generation or is_zero_block(unit.applied_text) != is_zero_block(
            tail.raw_text
        ):
            for widget in unit.widgets():
                await self._safe_remove(widget)
            self._tails[kind] = await self._build_unit(
                kind, kind, tail.raw_text, mount_before_tails=False
            )
            return

        if tail.raw_text == unit.applied_text:
            return
        # Growth of a fallback tail: no markdown reparse either way (that is
        # what "ends the growing-reparse work" means — there is no parser on
        # this path at all), but a full rebuild of the line widgets is
        # simpler and safer than patching the deque in place, and the
        # fallback path is already the degenerate-content escape valve, not
        # the common case. Every widget is dropped and rebuilt from the new
        # text.
        for widget in unit.widgets():
            await self._safe_remove(widget)
        self._tails[kind] = await self._build_unit(
            kind, kind, tail.raw_text, mount_before_tails=False
        )

    async def _safe_write(self, widget: EntryMarkdown | None, verb: str, text: str) -> None:
        """R16 clause 3: a stale write after the widget was condensed away
        or removed updates nothing and raises nothing.
        """
        if widget is None or not widget.is_mounted:
            return
        await getattr(widget, verb)(text)

    async def _safe_remove(self, widget: Widget) -> None:
        if widget.is_mounted:
            await widget.remove()

    # ── condensation over mixed units (KTD2) ────────────────────────────

    async def _condense(self, records: tuple[TranscriptEntryRecord, ...]) -> None:
        """Evict widgets from the top until the window starts at ``new_top``.

        :meth:`_compute_new_top` guarantees ``new_top`` never lands strictly
        inside a block-rendered (atomic) entry — it rounds such a boundary
        up to that entry's end — so the only entry this method ever needs
        to *partially* fold is a line-rendered one (ordinary or fallen-back),
        and at most one: the single entry, if any, whose span straddles
        ``new_top``. Every entry fully below ``new_top`` is evicted whole
        first, in commit order, oldest first.
        """
        new_top = self._compute_new_top(records)
        removed_top_height = 0

        while self._entry_order:
            entry_id = self._entry_order[0]
            record = self._record_for(records, entry_id)
            if record is None:
                break
            start, count = record.line_span
            end = start + count
            if end > new_top:
                break
            unit = self._entries[entry_id]
            for widget in unit.widgets():
                removed_top_height += max(1, widget.outer_size.height)
                await self._safe_remove(widget)
            self._entry_order.popleft()
            del self._entries[entry_id]

        if self._entry_order:
            entry_id = self._entry_order[0]
            record = self._record_for(records, entry_id)
            if record is not None:
                start, count = record.line_span
                end = start + count
                if start < new_top < end:
                    # Guaranteed line-rendered by _compute_new_top's contract.
                    unit = self._entries[entry_id]
                    retain = end - new_top
                    while len(unit.lines) > retain:
                        widget = unit.lines.pop(0)
                        removed_top_height += max(1, widget.outer_size.height)
                        await self._safe_remove(widget)

        self._top = max(self._top, new_top)
        self._pending_removed_height = removed_top_height

    def _record_for(
        self, records: tuple[TranscriptEntryRecord, ...], entry_id: int | TranscriptKind
    ) -> TranscriptEntryRecord | None:
        for record in records:
            if record.entry_id == entry_id:
                return record
        return None

    def _compute_new_top(self, records: tuple[TranscriptEntryRecord, ...]) -> int:
        """``desired_top`` in real accounted-content-line units (KTD2).

        Walks committed entries newest-to-oldest, accumulating accounted
        rows (content lines, plus one banner row per fallen-back entry)
        against :attr:`mount_cap`. The first entry that does not fully fit
        determines the fold boundary: a block-rendered entry rounds up
        (evicted whole); a fallen-back entry with any remaining budget for
        at least one content row keeps its newest rows plus exactly one
        banner (partial retention) and folds the rest; a fallen-back entry
        with zero budget for content rounds forward (evicted whole, banner
        included — a banner never stands alone); an ordinary line-rendered
        entry folds exactly the rows over budget, same as v0.1.

        The very newest entry, if it is a *block-rendered* atomic unit, is
        always kept whole and is not charged against the budget at all
        (KTD2's qualified newest-entry exemption) — a fallen-back newest
        entry gets no such exemption and folds like any line content.
        """
        if not records:
            return self._top
        budget = self.mount_cap
        newest = records[-1]
        newest_unit = self._entries.get(newest.entry_id)
        exempt_newest = newest_unit is not None and newest_unit.kind == "block"

        for index in range(len(records) - 1, -1, -1):
            record = records[index]
            unit = self._entries.get(record.entry_id)
            start, count = record.line_span
            if exempt_newest and record.entry_id == newest.entry_id:
                continue
            is_fallback = unit is not None and unit.kind == "line" and unit.is_fallback
            is_block = unit is not None and unit.kind == "block"
            weight = count + (1 if is_fallback else 0)
            if budget - weight >= 0:
                budget -= weight
                continue
            if is_block:
                return max(self._top, start + count)
            if is_fallback:
                available_for_content = budget - 1
                if available_for_content <= 0:
                    return max(self._top, start + count)
                retained = min(count, available_for_content)
                return max(self._top, start + (count - retained))
            # Ordinary line-rendered entry: fold exactly the overage.
            retained = max(0, budget)
            return max(self._top, start + (count - retained))
        return max(self._top, 0)

    # ── condensation banner ──────────────────────────────────────────────

    async def _render_condensed(self) -> None:
        if self._top == 0:
            if self._condensed is not None:
                await self._condensed.remove()
                self._condensed = None
            return
        text = literal_text(CONDENSED_TEMPLATE.format(count=self._top))
        if self._condensed is None:
            self._condensed = Static(text, markup=False, classes="transcript--condensed")
            await self.mount(self._condensed, before=0)
        else:
            self._condensed.update(text)

    def _restore_anchor(self) -> None:
        removed_top_height = self._pending_removed_height
        self._pending_removed_height = 0
        if self.follow:
            # `immediate=True` reads `max_scroll_y` synchronously, before
            # Textual's own layout pass has necessarily run over widgets
            # `await self.mount_all(...)` only just mounted -- `mount_all`
            # awaits the *mount*, not a settled layout, so `max_scroll_y`
            # can still read the pane's pre-mount height. Found live: a
            # batch of 60 newly mounted line widgets left `scroll_offset`
            # at (0, 0) instead of the bottom. `immediate=False` uses
            # Textual's own `call_after_refresh` -- deferred to run once the
            # layout has actually settled, which is what this widget needs
            # every time, not only for a batch this size.
            self.scroll_end(animate=False, immediate=False)
        elif removed_top_height:
            self.scroll_to(y=max(0, self.scroll_offset.y - removed_top_height), animate=False)

    # ── follow-bottom control ────────────────────────────────────────────

    def hold_anchor(self) -> None:
        self.follow = False

    def follow_bottom(self) -> None:
        self.follow = True
        self.scroll_end(animate=False, immediate=True)
