"""The selection model behind the picker dialog — pure, and deliberately so.

A picker is something the operator *operates*, and the thing being operated is
a highlight moving over a filtered list. That is a state machine, and a state
machine that lives inside a widget can only be tested by driving a terminal.
Qwen Code makes the same split and it is the reason its selection behaviour is
checkable at all: ``packages/cli/src/ui/hooks/useSelectionList.ts`` at v0.21.5
is a ``useReducer`` over ``MOVE_UP`` / ``MOVE_DOWN`` / ``SET_ACTIVE_INDEX`` /
``SELECT_CURRENT``, with the rendering left to ``BaseSelectionList``. Everything
below is that idea in Python, under ADR-0002 — the domain core never imports the
terminal framework, so none of this needs a screen.

**Every transition returns a new :class:`Selection`.** Nothing here mutates, so
a test can hold the before and after side by side, and the widget can compare
them to decide whether it needs to repaint at all.

**Navigation visits unselectable rows rather than skipping them, and this is a
disagreement with the prior art worth stating.** Qwen Code's ``findNextValidIndex``
steps over ``disabled`` items, which is right for its menus: five entries, one
greyed out, and the operator can see the whole list at once. Talaria's model
list runs to about a hundred rows behind a scrolling window, and a provider is
unauthenticated for a reason the operator can act on. Skipping would make those
rows unreachable *and* unexplained — the highlight would jump and nothing would
say why. So the highlight lands on them, and :attr:`Selection.refusal` carries
the sentence the dialog shows when the operator presses enter there. Honest
degradation over a silent skip, which is the same rule the rest of this
interface follows.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Protocol

#: Shown under the rows on a list stage. Names every key that does something,
#: because a modal that has taken the keyboard owes the operator a way out.
#: ``→``/``←`` are listed beside ``enter``/``esc`` rather than instead of them:
#: both pairs work, and an operator who reaches for the arrows should not have
#: to discover that by trying.
LIST_HINT = "↑↓ move · →/enter select · ←/esc back · type to filter · ctrl+q quit"

#: How many rows the dialog shows at once. Hermes's own model overlay uses
#: twelve (``ui-tui/src/components/modelPicker.tsx``: ``const VISIBLE = 12``)
#: and it is a good number for the same reason there and here — tall enough
#: that a filtered list usually fits entirely, short enough to leave the
#: transcript visible behind an overlay on an 80x24 terminal.
VISIBLE_ROWS = 12


@dataclass(frozen=True)
class Choice:
    """One row an operator can highlight.

    ``payload`` is what selection emits and is kept separate from ``label``
    because the two differ whenever the label is decorated: a model row labels
    itself with its provider for the operator's benefit and emits only the
    command text. Keeping them apart means the decoration can change without
    changing what gets sent.

    ``refusal`` is required to be non-empty when ``selectable`` is false, and
    :meth:`__post_init__` enforces it. An unselectable row with nothing to say
    is the exact failure this module is built to avoid — the highlight lands,
    enter does nothing, and the operator has no way to find out why.

    ``number`` is the row's number **in the other surface that shares this
    listing** — the one ``/models <n>`` and ``/profiles <n>`` take from the
    composer — and is ``0`` for a row that has none. It is carried explicitly
    rather than derived from the row's position because the two are not the
    same thing and were briefly rendered as though they were: the models under
    the third provider are positions one through ten on their own stage and
    rows fourteen through twenty-three of the listing, so a screen numbering
    them from one was inviting ``/models 1`` to select something else
    entirely. A row whose number is ``0`` renders without one, which is the
    honest rendering for a provider — no ``/models`` argument names a provider
    at all.
    """

    key: str
    label: str
    payload: str = ""
    detail: str = ""
    number: int = 0
    marked: bool = False
    selectable: bool = True
    refusal: str = ""

    def __post_init__(self) -> None:
        if not self.selectable and not self.refusal:
            raise ValueError(f"unselectable choice {self.key!r} carries no refusal reason")


def subsequence_score(haystack: str, needle: str) -> int | None:
    """Rank a filter match, or ``None`` when the characters are not all there.

    Deliberately a subsequence match rather than a substring one, so ``gpt20``
    finds ``gpt-oss:20b`` — an operator filtering a model list types the
    memorable characters, not a contiguous run of them. Hermes reaches the same
    place through ``ui-tui/src/lib/fuzzy.ts``; this is the small version of it.

    Lower is better, and the score is the span the match occupies: a needle
    whose characters sit close together in the haystack beats one whose
    characters are scattered across it. A tie is broken by the caller keeping
    the original listing order, which is why this returns a span rather than
    trying to rank exhaustively.
    """
    if not needle:
        return 0
    hay = haystack.casefold()
    first: int | None = None
    at = 0
    for ch in needle.casefold():
        found = hay.find(ch, at)
        if found < 0:
            return None
        if first is None:
            first = found
        at = found + 1
    return at - (first or 0)


@dataclass(frozen=True)
class Selection:
    """A filtered, windowed list with one row highlighted.

    ``active`` indexes into :attr:`visible`, never into :attr:`items`. Hermes
    made the same choice and left a comment explaining it, because the
    alternative — an index into the full list, translated on every read — puts
    a translation step in front of every use and gets one of them wrong
    eventually. The cost is that ``active`` has to be re-derived whenever the
    filter changes, which :meth:`_settled` does in one place.
    """

    items: tuple[Choice, ...] = ()
    filter_text: str = ""
    active: int = 0

    @classmethod
    def opened(cls, items: tuple[Choice, ...]) -> Selection:
        """The list as it first appears: highlight on the marked row, else the top.

        Constructing ``Selection(items=...)`` directly leaves the highlight at
        the top, which is right for a list with no current row and wrong for
        one that has it: the operator opens the picker already *on* something,
        and starting them at row one makes them find it again before they can
        see it. Hermes opens its provider stage the same way —
        ``ui-tui/src/components/modelPicker.tsx`` at ``f1470ec76`` sets
        ``providerIdx`` to ``Math.max(0, next.findIndex(p => p.is_current))``,
        which is this method's fallback rule as well.

        **Talaria goes one level further than Hermes does.** That file sets
        ``modelIdx`` to a flat ``0`` on entering the model stage, so the
        current model is not where its second level opens; here it is, because
        a provider's model list is the longer of the two and the row worth
        landing on is the one already in use.
        """
        for index, choice in enumerate(items):
            if choice.marked:
                return cls(items=items, active=index)
        return cls(items=items)

    # ── derived views ────────────────────────────────────────────────────

    @property
    def visible(self) -> tuple[Choice, ...]:
        """The rows the filter admits, best match first, listing order on ties."""
        if not self.filter_text.strip():
            return self.items
        scored: list[tuple[int, int, Choice]] = []
        for order, choice in enumerate(self.items):
            hay = f"{choice.label} {choice.detail}"
            score = subsequence_score(hay, self.filter_text.strip())
            if score is not None:
                scored.append((score, order, choice))
        scored.sort(key=lambda row: (row[0], row[1]))
        return tuple(choice for _, _, choice in scored)

    @property
    def active_choice(self) -> Choice | None:
        """The highlighted row, or ``None`` when the filter admits nothing."""
        rows = self.visible
        if not rows or not 0 <= self.active < len(rows):
            return None
        return rows[self.active]

    @property
    def empty(self) -> bool:
        """True when the filter has excluded everything — a state worth naming.

        A dialog showing no rows because the filter matched nothing and a
        dialog showing no rows because the gateway returned none are different
        situations with different remedies, and the caller needs to tell them
        apart to say the right thing.
        """
        return not self.visible

    def window(self, rows: int = VISIBLE_ROWS) -> tuple[tuple[Choice, ...], int]:
        """The visible slice and its offset, highlight kept near the middle.

        A direct port of Hermes's ``windowItems``/``windowOffset``
        (``ui-tui/src/components/overlayControls.tsx``). Centring rather than
        edge-scrolling means the operator can always see where they are going
        as well as where they have been.
        """
        shown = self.visible
        offset = max(0, min(self.active - rows // 2, len(shown) - rows))
        return shown[offset : offset + rows], offset

    # ── transitions, each returning a new Selection ──────────────────────

    def _settled(self, active: int) -> Selection:
        """Clamp ``active`` into the current visible list."""
        count = len(self.visible)
        if count == 0:
            return replace(self, active=0)
        return replace(self, active=max(0, min(active, count - 1)))

    def move(self, delta: int) -> Selection:
        """Step the highlight, stopping at the ends rather than wrapping.

        Not wrapping is the same call Hermes makes (its handler guards with
        ``sel > 0`` and ``sel < count - 1``). Wrapping saves a keystroke at the
        boundary and costs the operator the ability to tell, without reading,
        whether they are at the end of the list.
        """
        return self._settled(self.active + delta)

    def to_index(self, index: int) -> Selection:
        """Highlight a specific visible row; out-of-range values are ignored."""
        if not 0 <= index < len(self.visible):
            return self
        return replace(self, active=index)

    def typed(self, char: str) -> Selection:
        """Extend the filter and return to the best match.

        The highlight resets to the top on every keystroke because the ranking
        has changed underneath it; leaving it where it was would point it at a
        row that means something different now.
        """
        return replace(self, filter_text=self.filter_text + char, active=0)._settled(0)

    def backspaced(self) -> Selection:
        """Shorten the filter by one character."""
        return replace(self, filter_text=self.filter_text[:-1], active=0)._settled(0)

    def cleared(self) -> Selection:
        """Drop the filter, keeping the highlighted row highlighted.

        Preserving the row across a filter clear is Hermes's
        ``providerIndexAfterClearingFilter``, and it is worth the bookkeeping:
        the operator filtered *to* find this row, and dropping them back at the
        top of a hundred-row list discards the work that got them here.
        """
        held = self.active_choice
        wide = replace(self, filter_text="", active=0)
        if held is None:
            return wide
        for index, choice in enumerate(wide.visible):
            if choice.key == held.key:
                return replace(wide, active=index)
        return wide

    def restocked(self, items: tuple[Choice, ...]) -> Selection:
        """Replace the rows, keeping the highlight on the same row if it survives.

        Qwen Code's ``INITIALIZE`` does this through ``areItemsStructurallyEqual``
        and an ``activeKey`` lookup, for the reason a refetch exposes: the list
        arrives again on every reconnect, and an operator who has scrolled to a
        row does not expect a refresh to move them. Falling back to the marked
        row, then to the top, means a genuinely new list still opens somewhere
        sensible.
        """
        held = self.active_choice
        fresh = replace(self, items=items, active=0)
        if held is not None:
            for index, choice in enumerate(fresh.visible):
                if choice.key == held.key:
                    return replace(fresh, active=index)
        for index, choice in enumerate(fresh.visible):
            if choice.marked:
                return replace(fresh, active=index)
        return fresh._settled(0)


@dataclass(frozen=True)
class Stage:
    """One level of the dialog: a title, a hint, and the rows at that level.

    Stages are stacked rather than swapped so ``escape`` can pop back to the
    level above with the highlight where the operator left it. Hermes achieves
    the same by resetting to a named stage and re-deriving the index; a stack
    keeps the "where they left it" part without the re-derivation.

    Lives here rather than beside the widget so a picker source can build one
    without importing the terminal framework — ADR-0002 runs in this direction
    and only this one.
    """

    title: str
    selection: Selection
    hint: str = LIST_HINT


class PickerSource(Protocol):
    """What the dialog needs to know about the thing being picked.

    Splitting this out is what lets one dialog serve both ``/models`` (two
    levels — a provider, then that provider's models) and ``/profiles`` (one
    level), without the dialog knowing anything about either. It also keeps the
    descent rules pure and testable: a test can walk a source from root to
    payload with no screen involved.
    """

    def root(self) -> Stage:
        """The level the dialog opens on."""
        ...

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        """What choosing ``choice`` at ``depth`` does.

        Returns a :class:`Stage` to push when there is another level below, or
        the payload string to emit when the choice is final.
        """
        ...
