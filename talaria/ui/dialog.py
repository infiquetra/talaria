"""The picker as a modal dialog — arrows move, enter selects, typing filters.

This module exists because the first picker was not one. It rendered a numbered
list and selection meant typing ``/models 3``, which is a listing with a lookup
table bolted on. The operator's words on first live use, recorded in
``docs/engineering-journal/QUEUED.md``: "I expected it to work like the Hermes
TUI. ``/models`` would open up a dialog picker of some sort, not just a list I
now need to pick a number."

**What made the first attempt go wrong is worth keeping, because it was a
reasonable decision applied one step too far.** ``talaria/ui/palette.py`` rejects
a modal search box for the *command listing*, on the grounds that it "would put
a second focus owner in front of the composer". That is correct for a listing —
something the operator reads — and the picker was modelled on it. But a picker
is something the operator *operates*, and operating needs keys: the composer
owns ``enter`` as "send message" while a picker needs ``enter`` as "select the
highlighted row". The old picker dodged that collision by taking no focus at
all, and the numbered list was the price. A modal does not dodge it: while the
dialog is up it is the only focus owner, so ``enter`` is unambiguous, and when
it closes the caret goes back to the composer. Both prior arts settle it the
same way — Hermes's model overlay and Qwen Code's dialogs are modal.

**Where the behaviour comes from.** Read at the pins named below, as prior art
in ADR-0003's sense — documentation of behaviour, not a source tree to
translate.

* Qwen Code v0.21.5, ``packages/cli/src/ui/hooks/useSelectionList.ts``: the
  selection state machine, which is why :mod:`talaria.domain.selection` is a
  pure model this widget merely renders.
* Hermes ``ui-tui/src/components/modelPicker.tsx`` and
  ``ui-tui/src/components/overlayControls.tsx`` at ``f1470ec76``: staged
  navigation (providers, then that provider's models), type-to-filter with the
  filter scoped per stage, the centred scrolling window, and the layered
  ``escape`` — clear the filter first, then go back a stage, then close.

**Everything reaching the screen goes through** :func:`~talaria.ui.literal.literal_text`
**with markup off.** Model names, provider names and profile descriptions are
gateway-supplied, and R22's rule is that every byte reaching the screen is
untrusted regardless of who configured its source. The highlight is a CSS class
on the row, never markup in the row's text, so no styling decision ever depends
on parsing gateway output.
"""

from __future__ import annotations

from dataclasses import replace

from textual import events
from textual.app import ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Static

from talaria.domain.selection import (
    VISIBLE_ROWS,
    Choice,
    PickerSource,
    Selection,
    Stage,
)
from talaria.ui.literal import literal_text

#: Shown when the filter has excluded every row. Distinct from the empty-source
#: line below on purpose: this one is fixed by pressing backspace, that one is
#: not fixed by anything the operator does at the keyboard.
NO_MATCH = "(nothing matches the filter)"

#: Prefix for the line shown when enter lands on a row that cannot be chosen.
REFUSED_PREFIX = "cannot select — "


class PickerDialog(ModalScreen[str | None]):
    """A stack of selectable lists over the transcript.

    Dismisses with the chosen payload, or ``None`` when the operator backs out.
    The caller decides what a payload means — the dialog never dispatches
    anything itself, which keeps the "what gets sent to the gateway" decision
    in one place in the app rather than split across a widget.
    """

    DEFAULT_CSS = """
    PickerDialog {
        align: center middle;
    }
    PickerDialog > Vertical {
        width: auto;
        min-width: 44;
        max-width: 92;
        height: auto;
        max-height: 20;
        border: round $accent;
        background: $surface;
        padding: 0 1;
    }
    PickerDialog .dialog--title {
        color: $accent;
        text-style: bold;
    }
    PickerDialog .dialog--filter {
        color: $text-muted;
    }
    PickerDialog .dialog--row {
        color: $text;
    }
    PickerDialog .dialog--row.-active {
        background: $accent;
        color: $text;
        text-style: bold;
    }
    PickerDialog .dialog--row.-unselectable {
        color: $text-muted;
    }
    PickerDialog .dialog--hint {
        color: $text-muted;
    }
    PickerDialog .dialog--refusal {
        color: $warning;
        display: none;
    }
    PickerDialog .dialog--refusal.-said {
        display: block;
    }
    """

    def __init__(self, source: PickerSource, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._source = source
        self._stack: list[Stage] = [source.root()]
        self._refusal = ""
        self._title: Static | None = None
        self._filter: Static | None = None
        self._hint: Static | None = None
        self._refusal_line: Static | None = None
        self._body: Vertical | None = None
        self._rows: list[Static] = []

    # ── read access, so tests never reach through to the framework ───────

    @property
    def stage(self) -> Stage:
        """The level currently on screen."""
        return self._stack[-1]

    @property
    def depth(self) -> int:
        """How many levels down the operator has gone; the root is zero."""
        return len(self._stack) - 1

    @property
    def selection(self) -> Selection:
        return self.stage.selection

    @property
    def title_text(self) -> str:
        return "" if self._title is None else str(self._title.content)

    @property
    def filter_text(self) -> str:
        return "" if self._filter is None else str(self._filter.content)

    @property
    def hint_text(self) -> str:
        return "" if self._hint is None else str(self._hint.content)

    @property
    def refusal_text(self) -> str:
        return "" if self._refusal_line is None else str(self._refusal_line.content)

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def active_row_text(self) -> str:
        """The highlighted row's text, or empty when nothing is highlighted.

        Reads the CSS class rather than the model, so a test asserting on this
        is asserting what the operator can see and not what the dialog meant to
        draw. The two disagreeing is exactly the defect worth catching.
        """
        for row in self._rows:
            if row.has_class("-active"):
                return str(row.content)
        return ""

    # ── composition and paint ────────────────────────────────────────────

    def compose(self) -> ComposeResult:
        with Vertical():
            self._title = Static(literal_text(""), markup=False, classes="dialog--title")
            yield self._title
            self._filter = Static(literal_text(""), markup=False, classes="dialog--filter")
            yield self._filter
            self._body = Vertical()
            yield self._body
            self._refusal_line = Static(
                literal_text(""), markup=False, classes="dialog--refusal"
            )
            yield self._refusal_line
            self._hint = Static(literal_text(""), markup=False, classes="dialog--hint")
            yield self._hint

    async def on_mount(self) -> None:
        await self._repaint()

    def _row_text(self, choice: Choice) -> str:
        """One row: current-marker, the row's own number if it has one, label, detail.

        The number comes from :attr:`Choice.number` and never from the row's
        position on screen. Nothing requires typing it any more, and it is kept
        because ``/models <n>`` still accepts it from the composer and it is
        what an operator reads back to somebody else — but that is only worth
        four cells while the number on screen is the number that surface takes.
        A stage whose rows have no such number (providers) renders without one
        rather than numbering itself from the top, which is what made the
        earlier version wrong.
        """
        marker = "*" if choice.marked else " "
        number = f"{choice.number:>3}. " if choice.number else ""
        detail = f"  {choice.detail}" if choice.detail else ""
        return f"{marker}{number}{choice.label}{detail}"

    async def _repaint(self) -> None:
        """Redraw the rows for the current stage.

        Rebuilds rather than reconciles. The list is bounded at
        :data:`~talaria.domain.selection.VISIBLE_ROWS` rows, so a rebuild is
        cheap, and reconciling a list whose contents reorder under a filter is
        the kind of bookkeeping that goes wrong quietly.
        """
        if self._body is None or self._title is None or self._filter is None:
            return
        if self._hint is None or self._refusal_line is None:
            return

        self._title.update(literal_text(self.stage.title))

        selection = self.selection
        shown, offset = selection.window()
        self._filter.update(
            literal_text(f"filter: {selection.filter_text}" if selection.filter_text else "")
        )

        for row in self._rows:
            await row.remove()
        self._rows = []

        if selection.empty:
            empty = Static(literal_text(NO_MATCH), markup=False, classes="dialog--row")
            self._rows.append(empty)
            await self._body.mount(empty)
        else:
            for position, choice in enumerate(shown):
                index = offset + position
                classes = "dialog--row"
                if index == selection.active:
                    classes += " -active"
                if not choice.selectable:
                    classes += " -unselectable"
                row = Static(
                    literal_text(self._row_text(choice)),
                    markup=False,
                    classes=classes,
                )
                self._rows.append(row)
                await self._body.mount(row)

        self._refusal_line.update(literal_text(self._refusal))
        self._refusal_line.set_class(bool(self._refusal), "-said")
        self._hint.update(literal_text(self.stage.hint))

    async def _replace_selection(self, selection: Selection) -> None:
        self._stack[-1] = replace(self.stage, selection=selection)
        await self._repaint()

    # ── keys ─────────────────────────────────────────────────────────────

    async def on_key(self, event: events.Key) -> None:
        """Every key the dialog answers, handled in one place.

        Deliberately not split across ``BINDINGS``. A modal that owns the
        keyboard has to decide what happens to *every* key — including the
        printable ones, which go into the filter — and splitting that decision
        between a binding table and a handler means the answer depends on
        Textual's dispatch order. One handler, one answer, and a test can drive
        it by posting keys.

        **Movement is arrow keys and the page/home/end family, and nothing
        else, because every remaining candidate is spoken for.** Emacs-style
        ``ctrl+p``/``ctrl+n`` were tried and ``ctrl+p`` is taken by Textual's
        own command palette (``App.COMMAND_PALETTE_BINDING``), which opens over
        the dialog. Vim-style ``j``/``k`` are printable and belong to the
        filter — Qwen Code hits the same wall and carries a ``disableVimNav``
        flag for it. Hermes settles on arrows alone for the same reason, and so
        does this: every letter stays available for filtering, which is what an
        operator does with a hundred-row model list.

        **The horizontal arrows are second names for select and back**, so
        ``right`` does exactly what ``enter`` does and ``left`` exactly what
        ``escape`` does — including ``right`` closing the dialog on a final
        row and ``left`` closing it at the root. They are free to take because
        the filter is append-and-backspace with no caret in it; the day it
        grows one, ``left``/``right`` are the first two keys that collide, and
        this paragraph is where to start reading.
        """
        key = event.key
        selection = self.selection

        if key in ("escape", "left"):
            event.stop()
            await self._back()
            return

        if key == "up":
            event.stop()
            await self._move(-1)
            return

        if key == "down":
            event.stop()
            await self._move(1)
            return

        if key == "pageup":
            event.stop()
            await self._move(-VISIBLE_ROWS)
            return

        if key == "pagedown":
            event.stop()
            await self._move(VISIBLE_ROWS)
            return

        if key == "home":
            event.stop()
            await self._move(-len(selection.visible))
            return

        if key == "end":
            event.stop()
            await self._move(len(selection.visible))
            return

        if key in ("enter", "right"):
            event.stop()
            await self._choose()
            return

        if key == "backspace":
            event.stop()
            self._refusal = ""
            await self._replace_selection(selection.backspaced())
            return

        if key == "ctrl+u":
            event.stop()
            self._refusal = ""
            await self._replace_selection(selection.cleared())
            return

        character = event.character
        if character is not None and character.isprintable():
            event.stop()
            self._refusal = ""
            await self._replace_selection(selection.typed(character))

    async def _move(self, delta: int) -> None:
        self._refusal = ""
        await self._replace_selection(self.selection.move(delta))

    async def _back(self) -> None:
        """Escape, layered: clear a filter, then pop a stage, then close.

        Hermes's ordering, and the reason for it is that each layer undoes the
        most recent thing the operator did. Closing the whole dialog because
        they had typed a filter would throw away a stage they were still
        working in.
        """
        self._refusal = ""
        if self.selection.filter_text:
            await self._replace_selection(self.selection.cleared())
            return
        if self.depth > 0:
            self._stack.pop()
            await self._repaint()
            return
        self.dismiss(None)

    async def _choose(self) -> None:
        """Enter: descend a level, emit a payload, or say why neither happened."""
        choice = self.selection.active_choice
        if choice is None:
            return
        if not choice.selectable:
            self._refusal = f"{REFUSED_PREFIX}{choice.refusal}"
            await self._repaint()
            return
        outcome = self._source.descend(self.depth, choice)
        if isinstance(outcome, str):
            self.dismiss(outcome)
            return
        self._refusal = ""
        self._stack.append(outcome)
        await self._repaint()


# ── U4's confirm-before-steal dialog (OP2, KTD8) ──────────────────────────


#: The two rows, in the order they are drawn. Cancel is first, and first is
#: where the highlight opens: the destructive act is one deliberate keypress
#: away rather than the default, and an operator who hits ``enter`` on reflex
#: has cancelled rather than detached somebody.
CONFIRM_CANCEL_ROW = "cancel — leave the session where it is"
CONFIRM_PROCEED_ROW = "detach that client and focus this session here"

#: What the dialog says it is doing when neither row is chosen.
CONFIRM_HINT = "↑↓ move · enter choose · esc cancel"


class ConfirmDialog(ModalScreen[bool]):
    """A two-row confirmation over the transcript: cancel, or go ahead.

    Dismisses ``True`` only when the operator chose the proceed row, and
    ``False`` for every other exit — escape, the cancel row, or the dialog
    being dismissed by anything else. A caller therefore never has to
    distinguish "declined" from "closed", because for a destructive act they
    are the same answer.

    **Not a** :class:`PickerDialog` **with two rows**, and the difference is
    the filter: the picker sends every printable key into a type-to-filter
    box, which is right for a hundred-row model list and wrong for a question
    with two answers — a stray keystroke would empty the dialog rather than
    answering it. This modal answers exactly the keys below and ignores the
    rest.

    Every line reaches the screen through :func:`~talaria.ui.literal.literal_text`
    with markup off, including the body copy, because the body names a session
    whose title the gateway supplied (R23).
    """

    DEFAULT_CSS = """
    ConfirmDialog {
        align: center middle;
    }
    ConfirmDialog > Vertical {
        width: auto;
        min-width: 44;
        max-width: 92;
        height: auto;
        max-height: 20;
        border: round $warning;
        background: $surface;
        padding: 0 1;
    }
    ConfirmDialog .dialog--title {
        color: $warning;
        text-style: bold;
    }
    ConfirmDialog .dialog--body {
        color: $text;
    }
    ConfirmDialog .dialog--row {
        color: $text;
    }
    ConfirmDialog .dialog--row.-active {
        background: $accent;
        color: $text;
        text-style: bold;
    }
    ConfirmDialog .dialog--hint {
        color: $text-muted;
    }
    """

    def __init__(
        self,
        *,
        title: str,
        body: tuple[str, ...] = (),
        proceed_label: str = CONFIRM_PROCEED_ROW,
        cancel_label: str = CONFIRM_CANCEL_ROW,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._title_line = title
        self._body_lines = body
        self._labels = (cancel_label, proceed_label)
        #: 0 is cancel and it is where the dialog opens — see the row constants.
        self._active = 0
        self._title: Static | None = None
        self._hint: Static | None = None
        self._rows: list[Static] = []

    # ── read access, so tests never reach through to the framework ───────

    @property
    def title_text(self) -> str:
        return "" if self._title is None else str(self._title.content)

    @property
    def body_texts(self) -> tuple[str, ...]:
        return tuple(self._body_lines)

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def active_row_text(self) -> str:
        """The highlighted row, read off the CSS class rather than the model —
        the same reason :attr:`PickerDialog.active_row_text` does."""
        for row in self._rows:
            if row.has_class("-active"):
                return str(row.content)
        return ""

    # ── composition and paint ────────────────────────────────────────────

    def compose(self) -> ComposeResult:
        with Vertical():
            self._title = Static(
                literal_text(self._title_line), markup=False, classes="dialog--title"
            )
            yield self._title
            for line in self._body_lines:
                yield Static(literal_text(line), markup=False, classes="dialog--body")
            for index, label in enumerate(self._labels):
                row = Static(
                    literal_text(label),
                    markup=False,
                    classes="dialog--row -active" if index == 0 else "dialog--row",
                )
                self._rows.append(row)
                yield row
            self._hint = Static(
                literal_text(CONFIRM_HINT), markup=False, classes="dialog--hint"
            )
            yield self._hint

    def _repaint(self) -> None:
        for index, row in enumerate(self._rows):
            row.set_class(index == self._active, "-active")

    # ── keys ─────────────────────────────────────────────────────────────

    async def on_key(self, event: events.Key) -> None:
        """Arrows move, enter answers, escape cancels; everything else is ignored.

        No ``y``/``n`` shortcut and no ``left``/``right`` alias for choose: the
        act behind the proceed row takes another client's session away with no
        notification to it, so the only path to it is landing on the row and
        pressing enter. Every other key leaves the dialog exactly as it was
        rather than doing something adjacent.
        """
        key = event.key
        if key == "escape":
            event.stop()
            self.dismiss(False)
            return
        if key in ("up", "down"):
            event.stop()
            delta = -1 if key == "up" else 1
            self._active = max(0, min(len(self._labels) - 1, self._active + delta))
            self._repaint()
            return
        if key == "enter":
            event.stop()
            self.dismiss(self._active == 1)
            return
