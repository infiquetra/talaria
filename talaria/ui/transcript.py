"""The transcript pane: a bounded mount over an unbounded projection (KTD14).

**The bound is on widgets, not on content, and the two must not be confused.**
The domain transcript accumulates without eviction — KTD14 says so explicitly,
and QUEUED.md carries the deferred work item for changing that. What this widget
promises is narrower and mechanically checkable: at most
:data:`DEFAULT_MOUNT_CAP` line widgets exist at any moment, with everything
older folded into one condensed block. Content stays reachable through the
projection (``terminal_read`` serves every line whether or not it is mounted),
so "bounded" here means bounded *rendering cost*, and the results doc says that
in those words rather than claiming bounded memory.

**Why the unit is a line and not an entry.** ``TranscriptView`` publishes lines;
that is also what the terminal-read buffer serves, so a mounted-widget count and
a served-line count are directly comparable. Mounting per entry would make the
cap meaningless the moment one entry is a 4,000-line tool dump.

**The update is a diff, not a rebuild.** Committed lines never change, so the
pane keeps the previous line tuple and advances a stable index. The only region
it ever re-examines is the provisional tail — the in-flight streaming block —
which is why a 50,000-delta replay does not cost O(transcript) per 50ms tick.

**"Committed" is the projection's word, not a guess made from matching text.**
The stable index is clamped to :attr:`TranscriptView.committed_lines` for
exactly that reason. The provisional block sits *after* the committed lines, so
appending an entry while a turn is streaming pushes every provisional line down
by the length of the new entry. Two consecutive snapshots can therefore agree on
a provisional line by coincidence — the streaming text simply did not change
between those two ticks — and inferring "settled" from that agreement is wrong.
It was wrong here: the floor advanced into the streaming block on frame 31 of
the stress corpus while zero entries had been committed, and the pane was
misaligned from line 0 for the remaining 616 frames, rendering 274 lines against
275 projected with one line of conversation on screen nowhere.
"""

from __future__ import annotations

from collections import deque
from typing import Final

from textual.containers import VerticalScroll
from textual.widgets import Static

from talaria.domain.models import TranscriptKind
from talaria.domain.projection import TranscriptView
from talaria.ui.literal import literal_text
from talaria.ui.markdown import inline_markdown

#: KTD14's default. Overridable per-app so the gate can measure a smaller cap
#: without editing the source, but the shipped default is the number the
#: threshold is stated against.
DEFAULT_MOUNT_CAP: Final[int] = 500

#: Rendered in place of everything that has been unmounted. One widget, always,
#: so the collapse cannot itself grow the mount count.
CONDENSED_TEMPLATE: Final[str] = "── {count} earlier lines condensed (still readable by the agent)"

#: The entry kinds whose lines get inline markdown, and the only ones.
#:
#: Both are agent prose, which is the whole argument for styling them: a model
#: writes ``**like this**`` and means emphasis. The kinds left out are left out
#: for a reason each time. ``user`` is the operator's own typed text, and
#: echoing back something other than what they typed is its own small betrayal.
#: ``tool`` is program output — file contents, a directory listing, a diff —
#: where an asterisk is far more likely to be a glob or a C comment than a
#: request for italics, and restyling it would make the screen disagree with the
#: program that produced it. Everything else is Talaria's own wording.
MARKDOWN_KINDS: Final[frozenset[TranscriptKind]] = frozenset({"assistant", "reasoning"})


class TranscriptLine(Static):
    """One projected line, keeping the projection's text beside the drawn text.

    The two are no longer the same string. Inline markdown removes the
    delimiters of whatever it styles, so a line projected as ``**done**`` is
    drawn as ``done`` in bold — and every existing check that compares the pane
    against the projection means the *projected* string. Rather than weaken
    those checks to whatever survives rendering, each widget carries the line it
    was built from, so "is the pane showing the right line" and "what does the
    terminal actually paint" stay two separate, separately answerable questions.
    """

    def __init__(self, source: str, *, kind: TranscriptKind | None = None) -> None:
        renderable = inline_markdown(source) if kind in MARKDOWN_KINDS else literal_text(source)
        super().__init__(renderable, markup=False)
        #: The projection line, verbatim — not the text that ends up on screen.
        self.source = source


class TranscriptPane(VerticalScroll):
    """Scrollable, bounded, plain-text transcript."""

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
    """

    def __init__(self, *, mount_cap: int = DEFAULT_MOUNT_CAP, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.mount_cap = max(1, mount_cap)
        #: True while the pane pins itself to the newest line. Reading while
        #: scrolled away clears it, and streaming must not steal the view back.
        self.follow = True
        self._lines: tuple[str, ...] = ()
        self._stable = 0
        self._widgets: deque[TranscriptLine] = deque()
        #: Absolute index of the first mounted line, tracked directly rather
        #: than inferred from how many times a trim has run. It used to be
        #: inferred: a counter incremented on every left-hand eviction served as
        #: both "lines folded away" and "where the window starts". Those are the
        #: same number only if no line is ever evicted twice, and correct
        #: reconciliation evicts twice routinely — the provisional block is
        #: dropped from the right and re-derived, so lines cross the left edge
        #: again. On the 50,000-delta corpus the counter reached 7,493 for a
        #: transcript that only ever had 4,454 lines, which put the window at an
        #: index the projection does not have and made the pane render a wrong
        #: slice of a correct projection.
        self._top = 0
        self._condensed: Static | None = None
        #: Highest mounted-widget count observed. The gate reads this rather
        #: than sampling, so a spike between two samples cannot be missed.
        self.peak_mounted = 0

    # ── measurement surface ──────────────────────────────────────────────

    @property
    def mounted_count(self) -> int:
        """Widgets actually in the tree — ``len(self.children)``, not bookkeeping.

        This used to return ``len(self._widgets) + 1``, counting the pane's own
        private deque. The deque is what this class *believes* it has mounted,
        and nothing reconciled it against the real tree, so any widget that left
        the deque while staying mounted was invisible to the metric forever.
        Deleting the two ``widget.remove()`` calls left 4,455 ``Static`` widgets
        genuinely mounted — more than seven times the gate's 600 ceiling — while
        this property still reported 501 and the gate passed.

        A measurement the measured object supplies about itself is not a
        measurement. ``self.children`` is Textual's own record of the tree.
        """
        return len(self.children)

    @property
    def condensed_count(self) -> int:
        """Lines represented by the condensed block rather than by a widget.

        Read as a position, which is what makes ``condensed_count + mounted``
        equal the transcript length: it is the index the mounted window starts
        at, so it can fall as well as rise if the window is re-derived further
        up. A cumulative eviction tally cannot fall, and that is exactly how it
        came to exceed the number of lines that had ever existed.
        """
        return self._top

    @property
    def rendered_lines(self) -> tuple[str, ...]:
        """The projection lines this pane currently holds, in order.

        Read from the mounted widgets rather than from the pane's own index
        arithmetic, for the reason :attr:`mounted_count` gives: a number the
        measured object computes about itself is not a measurement. Each widget
        reports the line it was built from, so a pane that mounted the wrong
        slice still fails the comparison.

        This is the projection's text, not the terminal's. The two differ
        wherever inline markdown consumed a delimiter, and the checks that use
        this property — ``interface_shows_everything`` in the replay gate, the
        window assertions in the bounds suite — are asking whether the pane
        holds the right *content*, which is the projected string. For the drawn
        characters, read :attr:`drawn_lines`.
        """
        return tuple(widget.source for widget in self._widgets)

    @property
    def drawn_lines(self) -> tuple[str, ...]:
        """The characters the terminal actually paints, in order.

        Equal to :attr:`rendered_lines` except on agent prose carrying inline
        markdown, where the delimiters of a styled construct are gone. Nothing
        else is ever removed, and the suite asserts that as a property of the
        renderer rather than as a claim in a docstring.
        """
        return tuple(str(widget.content) for widget in self._widgets)

    # ── update ───────────────────────────────────────────────────────────

    async def apply(self, view: TranscriptView) -> None:
        """Reconcile the mounted widgets with a new projection snapshot."""
        current = view.lines
        stable = self._common_prefix(current)

        removed_top_height = 0
        # 1. Drop every mounted widget at or beyond the first divergence.
        #    Normally only the in-flight streaming block lands here, so the loop
        #    is short; when an entry commits mid-stream the whole provisional
        #    block is re-derived, which is the correct amount of work rather
        #    than an unlucky amount.
        while self._top + len(self._widgets) > stable and self._widgets:
            widget = self._widgets.pop()
            await widget.remove()

        # The divergence can also sit *below* the window, if a line that had
        # already been condensed changed. Nothing un-condenses it, but the
        # window must stop claiming a position the projection no longer has.
        if not self._widgets:
            self._top = min(self._top, stable)

        # 2. Condense from the top *before* mounting, never after, and by
        #    position rather than by repeated single-widget trims. The pane
        #    keeps the newest ``mount_cap`` lines, so the widget count cannot
        #    exceed the cap even for one frame. Trimming after the mount held
        #    the steady-state cap while transiently mounting 667 widgets against
        #    KTD14's ceiling of 600: a slow frame the operator can see and a
        #    snapshot taken afterwards cannot.
        desired_top = max(self._top, len(current) - self.mount_cap)
        while self._top < desired_top and self._widgets:
            widget = self._widgets.popleft()
            removed_top_height += max(1, widget.outer_size.height)
            await widget.remove()
            self._top += 1
        # Lines the window never reached are condensed without ever having been
        # a widget, which is the whole point of condensing before mounting.
        self._top = max(self._top, desired_top)

        # 3. Mount exactly the window's missing suffix. By construction
        #    ``len(current) - self._top <= mount_cap``, so this cannot overshoot.
        start = self._top + len(self._widgets)
        pending = list(current[start:])
        if pending:
            new_widgets = [
                TranscriptLine(line, kind=view.kind_at(start + offset))
                for offset, line in enumerate(pending)
            ]
            self._widgets.extend(new_widgets)
            await self.mount_all(new_widgets)
            # Sampled at the moment of maximum mount, which is now also the end
            # of the method — there is no later trim to hide a spike behind.
            # Keeping the sample here anyway, because a future edit that
            # reintroduces a trim should not silently turn this metric back into
            # an identity that can never exceed the cap whatever the pane does.
            self.peak_mounted = max(self.peak_mounted, self.mounted_count)

        await self._render_condensed()

        self._lines = current
        # The floor for the *next* scan is clamped to the committed boundary,
        # while the truncation above used the true divergence point. The two
        # differ on purpose. Truncating at the true divergence keeps unchanged
        # provisional widgets mounted, so a streaming delta churns one widget
        # rather than the whole block. Storing the true divergence as the floor
        # was the defect: a provisional line that merely *happened* to match
        # between two ticks was recorded as settled, and the scan never looked
        # at it again. It then moved -- appending an entry mid-stream pushes the
        # whole streaming block down, which the corpus does fifteen times -- and
        # the pane stayed misaligned from that point to the end of the session.
        # Committed lines are the only ones that can never move, so they are the
        # only ones the floor may cover.
        self._stable = min(stable, view.committed_lines)
        self.peak_mounted = max(self.peak_mounted, self.mounted_count)
        self._restore_anchor(removed_top_height)

    def _common_prefix(self, current: tuple[str, ...]) -> int:
        """How many leading lines are unchanged since the last snapshot.

        Scanning starts at the previously established stable index, which
        :meth:`apply` keeps at or below the committed boundary. The work per tick
        is therefore proportional to the newly committed lines plus the
        provisional streaming block, rather than to the length of the transcript
        — and every line that can still move is looked at every time.
        """
        index = min(self._stable, len(current), len(self._lines))
        limit = min(len(current), len(self._lines))
        while index < limit and current[index] == self._lines[index]:
            index += 1
        return index

    async def _render_condensed(self) -> None:
        # The block goes away when the window is re-derived far enough up that
        # nothing is below it any more. Leaving a "0 earlier lines condensed"
        # banner mounted would be both wrong on screen and an extra widget in
        # every count that is supposed to mean "lines".
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

    def _restore_anchor(self, removed_top_height: int) -> None:
        """Follow the bottom, or hold the reader's place (R38's anchor clause).

        When lines are unmounted off the top the content above the viewport
        shrinks, so an unadjusted scroll offset would jump the reader forward by
        exactly that much. Subtracting the removed height is what makes
        "scrolled away and reading" survive a condense.
        """
        if self.follow:
            self.scroll_end(animate=False, immediate=True)
        elif removed_top_height:
            self.scroll_to(y=max(0, self.scroll_offset.y - removed_top_height), animate=False)

    # ── follow-bottom control ────────────────────────────────────────────

    def hold_anchor(self) -> None:
        """Stop following the newest line — the operator is reading."""
        self.follow = False

    def follow_bottom(self) -> None:
        self.follow = True
        self.scroll_end(animate=False, immediate=True)
