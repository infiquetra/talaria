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
"""

from __future__ import annotations

from collections import deque
from typing import Final

from textual.containers import VerticalScroll
from textual.widgets import Static

from talaria.domain.projection import TranscriptView
from talaria.ui.literal import literal_text

#: KTD14's default. Overridable per-app so the gate can measure a smaller cap
#: without editing the source, but the shipped default is the number the
#: threshold is stated against.
DEFAULT_MOUNT_CAP: Final[int] = 500

#: Rendered in place of everything that has been unmounted. One widget, always,
#: so the collapse cannot itself grow the mount count.
CONDENSED_TEMPLATE: Final[str] = "── {count} earlier lines condensed (still readable by the agent)"


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
        self._widgets: deque[Static] = deque()
        self._condensed_count = 0
        self._condensed: Static | None = None
        #: Highest mounted-widget count observed. The gate reads this rather
        #: than sampling, so a spike between two samples cannot be missed.
        self.peak_mounted = 0

    # ── measurement surface ──────────────────────────────────────────────

    @property
    def mounted_count(self) -> int:
        """Line widgets currently mounted, including the condensed block."""
        return len(self._widgets) + (1 if self._condensed is not None else 0)

    @property
    def condensed_count(self) -> int:
        """Lines represented by the condensed block rather than by a widget."""
        return self._condensed_count

    @property
    def rendered_lines(self) -> tuple[str, ...]:
        """The text actually on screen, in order — what a snapshot test reads."""
        return tuple(str(widget.content) for widget in self._widgets)

    # ── update ───────────────────────────────────────────────────────────

    async def apply(self, view: TranscriptView) -> None:
        """Reconcile the mounted widgets with a new projection snapshot."""
        current = view.lines
        stable = self._common_prefix(current)

        removed_top_height = 0
        # 1. Drop the provisional tail that changed. Only the in-flight
        #    streaming block can land here, so this loop is short.
        while self._top_index + len(self._widgets) > stable and self._widgets:
            widget = self._widgets.pop()
            await widget.remove()

        start = self._top_index + len(self._widgets)
        pending = list(current[start:])

        # 2. A backlog larger than the cap is condensed *before* it is
        #    mounted, never after. Mounting 4,000 widgets and immediately
        #    removing 3,500 of them would satisfy the steady-state cap while
        #    briefly holding eight times it — a transient that a snapshot after
        #    the fact cannot see and a slow frame the operator can. Everything
        #    already mounted is going to fall off anyway in that case, so it is
        #    dropped first and the surplus new lines never become widgets.
        if len(pending) >= self.mount_cap:
            while self._widgets:
                widget = self._widgets.popleft()
                removed_top_height += max(1, widget.outer_size.height)
                self._condensed_count += 1
                await widget.remove()
            surplus = len(pending) - self.mount_cap
            if surplus > 0:
                self._condensed_count += surplus
                pending = pending[surplus:]

        # 3. Mount what is left, then enforce the cap incrementally.
        if pending:
            new_widgets = [Static(literal_text(line), markup=False) for line in pending]
            self._widgets.extend(new_widgets)
            await self.mount_all(new_widgets)
            # Sample the peak HERE, before the step-4 trim, not only at the end
            # of this method. Step 2's comment is right that a transient the
            # operator can see as a slow frame is the thing that matters — but a
            # peak sampled only after the trim cannot observe one, because the
            # trim has by then restored the cap. Measured after the trim this
            # metric can never exceed mount_cap + 1 whatever the pane does,
            # which makes it an identity rather than a measurement. The honest
            # peak is the post-mount count, and it is what the KTD14 gate reads.
            self.peak_mounted = max(self.peak_mounted, self.mounted_count)

        while len(self._widgets) > self.mount_cap:
            widget = self._widgets.popleft()
            removed_top_height += max(1, widget.outer_size.height)
            self._condensed_count += 1
            await widget.remove()
        if self._condensed_count:
            await self._render_condensed()

        self._lines = current
        self._stable = stable
        self.peak_mounted = max(self.peak_mounted, self.mounted_count)
        self._restore_anchor(removed_top_height)

    @property
    def _top_index(self) -> int:
        return self._condensed_count

    def _common_prefix(self, current: tuple[str, ...]) -> int:
        """How many leading lines are unchanged since the last snapshot.

        Scanning starts at the previously established stable index, so the work
        per tick is proportional to the size of the provisional streaming block
        rather than to the length of the transcript.
        """
        index = min(self._stable, len(current), len(self._lines))
        limit = min(len(current), len(self._lines))
        while index < limit and current[index] == self._lines[index]:
            index += 1
        return index

    async def _render_condensed(self) -> None:
        text = literal_text(CONDENSED_TEMPLATE.format(count=self._condensed_count))
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
