"""Where the caret goes when Talaria takes a control away.

The composer owns the caret. It is focused once at mount and nothing here
disputes that — an operator who presses ``tab`` onto the transcript, or clicks a
sub-agent row, has moved the caret deliberately and keeps it where they put it.

What this module is about is the other kind of move: the caret going somewhere
the operator did not send it, because the widget holding it stopped existing.
Textual handles that case itself, and its answer is wrong for this interface.
``Screen._reset_focus`` hands the caret to the neighbouring entry in the focus
chain, and the neighbour above every control Talaria mounts is the scrollable
region containing it. Both regions are ``VerticalScroll``, which sets
``can_focus = True`` so arrow keys scroll — a scroll container therefore accepts
the caret, discards every printable key sent to it, and draws no caret anywhere.

The result is an interface that has quietly stopped accepting text. Nothing is
greyed out, the composer still shows its placeholder because it is still empty,
and the app answers ``ctrl+c`` normally because those bindings are ``priority``
and never needed the caret. So the operator types a message, sees no characters
appear, and has nothing on screen to tell them why. The only reliable way out is
to quit and relaunch.

The rule this module enforces is one sentence: **when Talaria takes a control
away, the caret goes back to the composer.** A region announces the loss with
:class:`CaretReleased`; the app answers it by focusing the composer. Operator
focus moves are untouched, because no control was taken away in one.
"""

from __future__ import annotations

from textual.app import ScreenStackError
from textual.dom import NoScreen
from textual.message import Message
from textual.widget import Widget


class CaretReleased(Message):
    """A control that held the caret has been taken away.

    Posted *after* the removal rather than before it, and the ordering is not
    incidental. ``Widget.remove`` is awaited, and Textual reassigns focus from
    inside that call (``App._prune`` → ``Screen._reset_focus``). A message posted
    first would be delivered at the first suspension point — which is inside the
    removal — and the app's answer would then be overwritten by the framework's.
    Announcing afterwards means the app has the last word.
    """


def holds_caret(widget: Widget) -> bool:
    """True when this widget, or anything inside it, owns the caret.

    The descendant case is the one that matters: a prompt card never holds the
    caret itself, its ``Input`` or its buttons do, and asking only about the card
    would report every removal as harmless.

    A widget with no screen cannot hold the caret, and asking is not an error —
    that is simply the state of a widget built but not yet mounted, which the
    reconciling passes construct as a matter of course.
    """
    try:
        focused = widget.screen.focused
    except (NoScreen, ScreenStackError):
        return False
    if focused is None:
        return False
    return focused is widget or widget in focused.ancestors
