"""The bordered multi-line composer (KTD4, R10–R12).

``Input`` is single-line, so R12 eliminates it before the comparison starts;
``TextArea`` is the only framework-provided multi-line editor R11's paste and
wide-character requirements permit. Configuration is fixed by KTD4:
``language=None`` (no syntax engine to re-interpret a chat message),
``soft_wrap=True``, ``show_line_numbers=False``, and a placeholder that
documents the two bindings — discoverability is a requirement, not a nicety.

**Enter submits and Ctrl+J inserts a newline.** Enter-submits matches Hermes and
every chat interface. The newline key is Ctrl+J rather than Shift+Enter because
Ctrl+J is a plain line feed that every terminal in the supported matrix can
deliver, while Shift+Enter needs the kitty keyboard protocol the matrix does not
assume.

**In replay mode Enter must not echo.** There is no gateway, so nothing can be
sent. The tempting local behaviour — drop the composed text into the transcript
— produces a line that is indistinguishable on screen from a message that was
actually delivered, which is the exact confusion AE11's inert-control rule
exists to prevent. So the composer keeps the text, renders the refusal notice,
and writes nothing to the transcript.
"""

from __future__ import annotations

from typing import Final

from textual import events
from textual.app import ComposeResult
from textual.containers import Vertical
from textual.message import Message
from textual.widgets import Static, TextArea

from talaria.ui.literal import literal_text

#: Shown in the empty composer. Carries both bindings because R12 asks that
#: "submit versus newline" be discoverable without documentation.
PLACEHOLDER: Final[str] = "Message  ·  Enter sends  ·  Ctrl+J newline"


class ChatTextArea(TextArea):
    """``TextArea`` with Enter rebound to submit and Ctrl+J to newline.

    The rebinding is done by intercepting the key rather than by editing
    ``BINDINGS``: Textual's ``TextArea`` inserts a newline for Enter inside
    ``_on_key`` (it is treated as an insert, not as a binding), so a binding
    entry alone would be shadowed and the widget would keep inserting.
    """

    class Submitted(Message):
        """Enter was pressed on composed text."""

        def __init__(self, composer: ChatTextArea, text: str) -> None:
            super().__init__()
            self.composer = composer
            self.text = text

    async def _on_key(self, event: events.Key) -> None:
        if event.key == "enter":
            event.stop()
            event.prevent_default()
            self.post_message(self.Submitted(self, self.text))
            return
        if event.key in ("ctrl+j", "shift+enter"):
            event.stop()
            event.prevent_default()
            self.insert("\n")
            return
        await super()._on_key(event)


class Composer(Vertical):
    """The bordered region R10 requires stay visible while the transcript streams."""

    DEFAULT_CSS = """
    Composer {
        height: auto;
        max-height: 12;
        border: round $accent;
        border-title-align: left;
        padding: 0 1;
    }
    Composer > ChatTextArea {
        height: auto;
        max-height: 8;
        padding: 0;
    }
    Composer > .composer--notice {
        height: 1;
        color: $warning;
        /* One row, and the row is *routinely* too narrow for the line. Every
           honest delivery note names both what happened and what to do about
           it, so the operative clause is often past column 60 — and a plain
           one-row Static clips it with nothing on screen to say it clipped. A
           sentence that stops mid-clause reads as a sentence that ended, which
           is the same silent-truncation failure the command body's overflow
           marker exists to prevent. ``text-overflow: ellipsis`` is only
           reachable with wrapping off: with wrapping on the widget folds the
           tail onto a second row that ``height: 1`` then hides, and nothing is
           ever marked. */
        text-wrap: nowrap;
        text-overflow: ellipsis;
    }
    """

    def __init__(self, *, notice: str = "", **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._initial_notice = notice
        self._notice_widget: Static | None = None
        self._text_area: ChatTextArea | None = None

    def compose(self) -> ComposeResult:
        # ``compact`` rather than ``border: none`` in this class's CSS, and the
        # difference is two rows of jitter. ``TextArea`` re-declares
        # ``border: tall`` inside its own ``&:focus`` block, which outranks a
        # descendant selector written here — so the editor was three rows while
        # focused and one row while not, and the whole interface above it jumped
        # by two the instant focus moved. That is not only ugly: a mouse press on
        # a prompt button moves focus here away, the layout shifts under the
        # cursor between the press and the release, and the click lands on
        # whatever slid into that row. ``compact`` sets ``-textual-compact``,
        # whose ``!important`` wins in both states, so the height stops
        # depending on focus at all.
        self._text_area = ChatTextArea(
            "",
            language=None,
            soft_wrap=True,
            show_line_numbers=False,
            placeholder=PLACEHOLDER,
            id="composer-input",
            compact=True,
        )
        yield self._text_area
        self._notice_widget = Static(
            literal_text(self._initial_notice), markup=False, classes="composer--notice"
        )
        yield self._notice_widget

    def on_mount(self) -> None:
        self.border_title = "compose"

    # ── text access, so tests never reach through to the framework ───────

    @property
    def text_area(self) -> ChatTextArea:
        if self._text_area is None:  # pragma: no cover - compose always runs first
            raise RuntimeError("composer queried before it was composed")
        return self._text_area

    @property
    def text(self) -> str:
        return self.text_area.text

    @text.setter
    def text(self, value: str) -> None:
        self.text_area.text = value

    @property
    def submitted_text(self) -> str:
        """What a live submit would actually send: the text with edges trimmed.

        Trailing whitespace is an artifact of typing, not content — but the
        *interior* is left alone, because a pasted code block's indentation is
        exactly the thing an operator would be furious to have silently
        reformatted.
        """
        return self.text.strip()

    def clear(self) -> None:
        """Empty the editor after a message has actually left (R3).

        Only called once a submit is known to have been delivered or is known to
        be recorded in the transcript. A composer cleared on a refused or failed
        send loses what the operator typed, which is the one thing a chat client
        must never do.
        """
        self.text_area.text = ""

    @property
    def notice(self) -> str:
        if self._notice_widget is None:
            return ""
        return str(self._notice_widget.content)

    def show_notice(self, message: str) -> None:
        """Render a refusal or status line beneath the editor, keeping the text."""
        if self._notice_widget is not None:
            self._notice_widget.update(literal_text(message))
