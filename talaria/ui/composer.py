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

**Large pastes are inserted first and collapsed second (KTD16, AE13).** At or
above six lines or 512 bytes the composer asks the gateway to spool the text to
a file and hand back a one-line placeholder. The order matters: the literal
text goes in the moment the paste arrives, exactly as KTD4 specifies for a
small one, and the placeholder replaces it only once a placeholder actually
exists. The obvious alternative — hold the text, insert nothing, wait for the
round trip — leaves the operator's paste in no visible place at all for the
duration of an RPC, and leaves it nowhere at all if the RPC fails. Here a
failure is simply a paste that did not collapse: the full text is still in the
editor, still editable, and the notice says the capability was not there.
"""

from __future__ import annotations

from typing import Final

from textual import events
from textual.app import ComposeResult, ScreenStackError
from textual.containers import Vertical
from textual.dom import NoScreen
from textual.message import Message
from textual.widgets import Static, TextArea

from talaria.domain.commands import PasteThreshold
from talaria.domain.composer_history import abandon, move_down, move_up
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

    class LargePaste(Message):
        """A paste tripped KTD16's threshold and is already in the editor.

        Carries the pasted body so the app can send it, and nothing else: the
        app does not need to know where in the document it landed, because the
        swap is done by :meth:`Composer.collapse_paste` against the text as it
        stands at that moment rather than against a remembered offset.
        """

        def __init__(self, composer: ChatTextArea, text: str) -> None:
            super().__init__()
            self.composer = composer
            self.text = text

    def __init__(self, *args: object, **kwargs: object) -> None:
        super().__init__(*args, **kwargs)  # type: ignore[arg-type]
        #: KTD16's bounds. Replaced by :class:`Composer` from configuration.
        self.paste_threshold = PasteThreshold()

    async def _on_paste(self, event: events.Paste) -> None:
        """Insert the paste literally, then ask for a collapse if it is large.

        ``super()`` runs first and unconditionally, so KTD4's literal insert is
        the floor for every paste — including one whose collapse is about to
        fail, and including one arriving in replay where there is no gateway to
        collapse it.

        **``prevent_default()`` is load-bearing and is not about the default
        action.** Textual dispatches a message to the handler defined in *every*
        class of the receiver's MRO, not just the most derived one
        (``message_pump.py:758-798``: ``for cls in self.__class__.__mro__ … yield
        cls.__dict__.get(f"_{method_name}")``). So defining ``_on_paste`` here
        does not replace ``TextArea._on_paste``; it runs in addition to it, and
        every paste was inserted twice — measured as 798 newlines from a
        400-line paste. ``_no_default_action`` is the flag that loop breaks on,
        which makes ``prevent_default`` the way to say "this override *is* the
        handler". The same hazard is why :meth:`_on_key`'s two intercepted keys
        call it; the difference is that ``TextArea._on_key`` sets it itself for
        ordinary keys and ``TextArea._on_paste`` never does.
        """
        event.prevent_default()
        await super()._on_paste(event)
        # After the literal insert, sync the slash palette for a user paste.
        # Collapse-placeholder replacement is programmatic via Composer.text
        # assignment and must not open the palette, so it is not handled here.
        # For a user paste, the document already reflects the new text, so we
        # can sync directly with self.text.
        try:
            app = self.app
            palette = app.palette  # type: ignore[attr-defined]
            catalog = getattr(app, "catalog", None)
            if self.has_focus or self._has_composer_focus():
                await palette.sync_slash(catalog, self.text)
            else:
                if palette.is_slash_active:
                    await palette.hide_slash()
        except (NoScreen, ScreenStackError, AttributeError):
            pass
        if event.text and self.paste_threshold.trips(event.text):
            self.post_message(self.LargePaste(self, event.text))

    async def _on_key(self, event: events.Key) -> None:
        # ── palette seam (C1/C2, ruling 1 & 3) ─────────────────────────
        # Both units claim keys in ChatTextArea._on_key, not in
        # TalariaApp.on_key. One handler site, one ordered predicate — grep
        # for _on_key must show a single site. The palette (C2) claims
        # Up/Down/Enter/Esc/Tab while its filtered mode is active; history
        # (C1) is inert while the palette is open. The palette opens on
        # typed input, never on programmatic writes to composer.text (ruling
        # 3), so history recall's direct text assignment must not open it.
        # The palette's active state lives in PaletteRegion (is_slash_active),
        # driven only by typed-input paths that compute the predicted text.
        if self._is_slash_palette_open() and event.key in (
            "up",
            "down",
            "enter",
            "escape",
            "tab",
        ):
            # Do not delegate to super — TextArea would insert a newline for
            # Enter or move the caret for Up/Down, which is the defect C1
            # was repaired for (F3 listing left Enter to super and inserted a
            # newline instead of submitting).
            try:
                app = self.app
                palette = app.palette  # type: ignore[attr-defined]
            except (NoScreen, ScreenStackError, AttributeError):
                await super()._on_key(event)
                return
            if event.key == "up":
                palette.move_selection(-1)
                event.stop()
                event.prevent_default()
                return
            if event.key == "down":
                palette.move_selection(1)
                event.stop()
                event.prevent_default()
                return
            if event.key == "enter":
                entry = palette.selected_entry
                if entry is not None:
                    # Insert canonical name with trailing space, never dispatch.
                    catalog = getattr(app, "catalog", None)
                    if catalog is not None:
                        canon = catalog.canonical(entry.name)
                    else:
                        canon = entry.name
                    canon = canon.lstrip("/")
                    text = f"/{canon} "
                    self.text = text
                    try:
                        self.cursor_location = (0, len(text))
                    except (ValueError, AttributeError, NoScreen, ScreenStackError):
                        pass
                    try:
                        self.focus()
                    except (NoScreen, ScreenStackError, AttributeError):
                        pass
                    await palette.hide_slash()
                # When no match, keep text as-is and keep palette open
                # (zero-match handling). Either way, consume the key so
                # it does not submit or insert a newline.
                event.stop()
                event.prevent_default()
                return
            if event.key == "escape":
                await palette.hide_slash()
                event.stop()
                event.prevent_default()
                return
            if event.key == "tab":
                # Consumed to keep focus in the composer while open.
                event.stop()
                event.prevent_default()
                return
        # ── escape abandons a history walk but keeps the stash (KTD3) ─
        # Card-scoped escape (prompts.py:669) fires only when a card's
        # control holds the caret, never when the composer does, so there
        # is no conflict. Abandon keeps draft_stash for the next walk;
        # Down past newest is the restore path, Escape is the abandon path.
        # Palette has priority: when both palette and walk are active, Esc
        # closes the palette (above) and does not abandon the walk. Both
        # being true at once requires the composer text to have become a
        # slash prefix via typed input mid-walk, which the palette's sync
        # allows.
        if event.key == "escape":
            try:
                history = self.app.composer_history  # type: ignore[attr-defined]
            except (NoScreen, ScreenStackError, AttributeError):
                history = None
            if history is not None and history.index is not None:
                self.app.composer_history = abandon(history)  # type: ignore[attr-defined]
                event.stop()
                event.prevent_default()
                return
            # Not navigating — let the key bubble (prompt cards, app).
            await super()._on_key(event)
            return
        # ── history recall (C1, KTD2) ──────────────────────────────────
        # Single-line drafts always recall; multi-line drafts recall only at the
        # respective caret boundary so editing a paragraph still works.
        if event.key in ("up", "down"):
            try:
                history = self.app.composer_history  # type: ignore[attr-defined]
            except (NoScreen, ScreenStackError, AttributeError):
                history = None
            if history is not None:
                text = self.text
                has_newline = "\n" in text
                row, _col = self.cursor_location
                total_rows = text.count("\n") + 1 if text else 1
                if event.key == "up":
                    caret_at_top = True if not has_newline else row == 0
                    if not caret_at_top:
                        # Caret inside multi-line text — move caret, close palette if open
                        try:
                            palette = self.app.palette  # type: ignore[attr-defined]
                            if palette.is_slash_active:
                                await palette.hide_slash()
                        except (NoScreen, ScreenStackError, AttributeError):
                            pass
                        await super()._on_key(event)
                        return
                    new_state, new_text = move_up(history, text, True)
                    if new_text is not None:
                        self.app.composer_history = new_state  # type: ignore[attr-defined]
                        self.text = new_text
                        # Recalled text is editable with caret at the end (KTD4).
                        lines = new_text.split("\n")
                        last_row = len(lines) - 1
                        last_col = len(lines[last_row])
                        self.cursor_location = (last_row, last_col)
                        event.stop()
                        event.prevent_default()
                        # Programmatic write — do NOT sync slash (ruling 3).
                        return
                    # No history move (empty or at oldest) — let caret move and close palette
                    try:
                        palette = self.app.palette  # type: ignore[attr-defined]
                        if palette.is_slash_active:
                            await palette.hide_slash()
                    except (NoScreen, ScreenStackError, AttributeError):
                        pass
                    await super()._on_key(event)
                    return
                else:  # "down"
                    caret_at_bottom = True if not has_newline else row == total_rows - 1
                    if not caret_at_bottom:
                        try:
                            palette = self.app.palette  # type: ignore[attr-defined]
                            if palette.is_slash_active:
                                await palette.hide_slash()
                        except (NoScreen, ScreenStackError, AttributeError):
                            pass
                        await super()._on_key(event)
                        return
                    new_state, new_text = move_down(history, text, True)
                    if new_text is not None:
                        self.app.composer_history = new_state  # type: ignore[attr-defined]
                        self.text = new_text
                        lines = new_text.split("\n")
                        last_row = len(lines) - 1
                        last_col = len(lines[last_row])
                        self.cursor_location = (last_row, last_col)
                        event.stop()
                        event.prevent_default()
                        # Programmatic write — do not sync slash.
                        return
                    try:
                        palette = self.app.palette  # type: ignore[attr-defined]
                        if palette.is_slash_active:
                            await palette.hide_slash()
                    except (NoScreen, ScreenStackError, AttributeError):
                        pass
                    await super()._on_key(event)
                    return
        if event.key == "enter":
            event.stop()
            event.prevent_default()
            self.post_message(self.Submitted(self, self.text))
            return
        if event.key in ("ctrl+j", "shift+enter"):
            event.stop()
            event.prevent_default()
            self.insert("\n")
            # Newline breaks the slash predicate, so close palette if open
            try:
                palette = self.app.palette  # type: ignore[attr-defined]
                if palette.is_slash_active:
                    await palette.hide_slash()
            except (NoScreen, ScreenStackError, AttributeError):
                pass
            return
        # ── generic typed input and caret movement (KTD2 key path) ──────
        # TextArea updates its document *after* super()._on_key returns, so
        # reading self.text there gives the text before the key. Compute it.
        # The palette only matches lstrip text == "/" or "/name" with no spaces,
        # so the resulting text is trivial to derive when the widget is
        # single-line and the caret is at the end (the palette closes on any
        # caret move, so if it was open the caret was at the end).
        current = self.text
        key = event.key
        predicted: str | None = None
        is_caret_move = False
        # caret-moving keys — text unchanged, palette closes
        caret_keys = {
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
            "ctrl+left",
            "ctrl+right",
            "shift+left",
            "shift+right",
            "shift+home",
            "shift+end",
            "ctrl+home",
            "ctrl+end",
            "ctrl+a",
            "ctrl+e",
        }
        # Check for caret keys including shifted variants via substring
        if key in caret_keys or any(
            sub in key for sub in ("left", "right", "home", "end", "pageup", "pagedown")
        ):
            # Up/down already handled above (history or palette), but left/right etc close
            is_caret_move = True
        elif event.character is not None and event.is_printable:
            # Printable character insertion at cursor
            try:
                row, col = self.cursor_location
                # Single-line: insert at col
                if "\n" not in current and row == 0:
                    predicted = current[:col] + event.character + current[col:]
                else:
                    predicted = current + event.character
            except (AttributeError, ValueError, IndexError):
                predicted = current + event.character
        elif key in ("backspace", "ctrl+h"):
            try:
                row, col = self.cursor_location
                if "\n" not in current and row == 0 and col > 0:
                    predicted = current[:col - 1] + current[col:]
                elif current:
                    predicted = current[:-1]
                else:
                    predicted = ""
            except (AttributeError, ValueError, IndexError):
                predicted = current[:-1] if current else ""
        elif key in ("delete", "ctrl+d"):
            try:
                row, col = self.cursor_location
                if "\n" not in current and row == 0 and col < len(current):
                    predicted = current[:col] + current[col + 1 :]
                elif current:
                    # At end, delete behaves like backspace
                    predicted = current[:-1] if current else ""
                else:
                    predicted = ""
            except (AttributeError, ValueError, IndexError):
                predicted = current[:-1] if current else ""
        elif key in ("space",):
            try:
                row, col = self.cursor_location
                if "\n" not in current and row == 0:
                    predicted = current[:col] + " " + current[col:]
                else:
                    predicted = current + " "
            except (AttributeError, ValueError, IndexError):
                predicted = current + " "
        else:
            # Unknown non-printable key — honest default is to close the palette
            # rather than leave it open with stale filter. This covers keys like
            # "f5", "ctrl+s" (which bubbles to the app's priority interrupt), etc.
            is_caret_move = True

        if is_caret_move:
            try:
                palette = self.app.palette  # type: ignore[attr-defined]
                catalog = getattr(self.app, "catalog", None)
                # has_focus check: only hide if we own caret, otherwise hide anyway
                if palette.is_slash_active:
                    await palette.hide_slash()
            except (NoScreen, ScreenStackError, AttributeError):
                pass
            await super()._on_key(event)
            return

        # Text-changing key: delegate to TextArea then sync palette with predicted
        # Predicted was computed; if None (should not happen here) use current + char fallback
        if predicted is None:
            # Should not happen, but treat as caret close
            await super()._on_key(event)
            return
        await super()._on_key(event)
        # Sync palette based on predicted text, not self.text (which may still be old)
        try:
            palette = self.app.palette  # type: ignore[attr-defined]
            catalog = getattr(self.app, "catalog", None)
            # Only sync when we own the caret; otherwise hide
            if self.has_focus or self._has_composer_focus():
                await palette.sync_slash(catalog, predicted)
            else:
                if palette.is_slash_active:
                    await palette.hide_slash()
        except (NoScreen, ScreenStackError, AttributeError):
            pass
        return

    def _has_composer_focus(self) -> bool:
        """Fallback focus ownership check when has_focus is stale."""
        try:
            focused = self.app.focused
            if focused is self:
                return True
            if focused is not None and self in getattr(focused, "ancestors", []):
                return True
        except (NoScreen, ScreenStackError, AttributeError):
            return False
        return False

    async def on_blur(self, event: events.Blur) -> None:
        for ancestor in self.ancestors:
            if isinstance(ancestor, Composer):
                ancestor.show_caret_location(False)
                break
        try:
            app = self.app
            palette = app.palette  # type: ignore[attr-defined]
            if palette.is_slash_active:
                await palette.hide_slash()
        except (NoScreen, ScreenStackError, AttributeError):
            pass

    async def on_focus(self, event: events.Focus) -> None:
        # Focus alone does not open the palette — only typed input does
        # (ruling 3). This handler is intentionally a no-op for opening;
        # closing is handled by on_blur. Keeping it as a method ensures the
        # focus event is consumed and not bubbled, but it does not sync.
        for ancestor in self.ancestors:
            if isinstance(ancestor, Composer):
                ancestor.show_caret_location(True)
                break

    def _is_slash_palette_open(self) -> bool:
        """Whether C2's slash-command palette claims Up/Down/Enter/Esc/Tab.

        While the filtered palette is active it claims those five keys and
        history is inert. Not keyed to the F3 browse listing (PaletteRegion
        showing as browse), which claims none of those keys. The active flag
        lives in PaletteRegion and is driven only by typed input via
        sync_slash, so programmatic writes (history recall, paste-collapse)
        never open it — ruling 3.
        """
        try:
            app = self.app
            palette = app.palette  # type: ignore[attr-defined]
            if not self.has_focus:
                # Palette is non-modal and never takes focus; if we don't have
                # the caret, we don't claim.
                try:
                    focused = app.focused
                    if focused is not self and self not in getattr(focused, "ancestors", []):
                        return False
                except (NoScreen, ScreenStackError, AttributeError):
                    return False
            return bool(palette.is_slash_active)
        except (NoScreen, ScreenStackError, AttributeError):
            return False


class Composer(Vertical):
    """The bordered region R10 requires stay visible while the transcript streams."""

    DEFAULT_CSS = """
    Composer {
        height: auto;
        max-height: 12;
        border: round $talaria-border-muted;
        border-title-align: left;
        padding: 0 1;
    }
    Composer.-caret-here {
        border: round $talaria-focus;
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

    def __init__(
        self,
        *,
        notice: str = "",
        paste_threshold: PasteThreshold | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._initial_notice = notice
        self._notice_widget: Static | None = None
        self._text_area: ChatTextArea | None = None
        self.paste_threshold = (
            paste_threshold if paste_threshold is not None else PasteThreshold()
        )

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
        self._text_area.paste_threshold = self.paste_threshold
        yield self._text_area
        self._notice_widget = Static(
            literal_text(self._initial_notice), markup=False, classes="composer--notice"
        )
        yield self._notice_widget

    def on_mount(self) -> None:
        self.show_caret_location(False)
        self.call_after_refresh(self._sync_caret_location)

    def _sync_caret_location(self) -> None:
        self.show_caret_location(self.text_area.has_focus)

    def show_caret_location(self, caret_here: bool) -> None:
        """Update the two focus cues without changing Composer geometry."""
        self.border_title = (
            "compose [*] caret here" if caret_here else "compose [ ] caret elsewhere"
        )
        self.set_class(caret_here, "-caret-here")

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
        # Programmatic write — must not open palette (ruling 3). Hide palette
        # if it was open; no timer is used. The write itself is synchronous,
        # the hide is scheduled as a task without a timing window.
        self.text_area.text = value
        try:
            pal = self.text_area.app.palette  # type: ignore[attr-defined]
            if pal.is_slash_active:
                import asyncio

                try:
                    asyncio.create_task(pal.hide_slash())
                except (RuntimeError, AttributeError, NoScreen, ScreenStackError):
                    pass
        except (NoScreen, ScreenStackError, AttributeError):
            pass

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
        try:
            pal = self.text_area.app.palette  # type: ignore[attr-defined]
            if pal.is_slash_active:
                import asyncio

                try:
                    asyncio.create_task(pal.hide_slash())
                except (RuntimeError, AttributeError, NoScreen, ScreenStackError):
                    pass
        except (NoScreen, ScreenStackError, AttributeError):
            pass

    @property
    def notice(self) -> str:
        if self._notice_widget is None:
            return ""
        return str(self._notice_widget.content)

    def show_notice(self, message: str) -> None:
        """Render a refusal or status line beneath the editor, keeping the text."""
        if self._notice_widget is not None:
            self._notice_widget.update(literal_text(message))

    # ── paste collapse (KTD16, AE13) ─────────────────────────────────────

    def collapse_paste(self, original: str, placeholder: str) -> bool:
        """Swap one pasted body for the gateway's placeholder, or leave it be.

        Returns whether the swap happened. It is a search-and-replace over the
        current text rather than a splice at a remembered offset because the
        offset is not trustworthy by the time the reply lands: the round trip
        spans an RPC, during which the operator may have typed above the paste,
        pasted again, or deleted part of it. A stale offset would quietly
        overwrite whatever had moved into those columns.

        A body that is no longer present — the operator edited or deleted it —
        means the swap does not happen and the caller says so. Not finding it is
        the correct outcome there: the alternative is appending a placeholder
        for text that is no longer in the composer, which would submit a
        reference to a file whose contents nobody is looking at.
        """
        current = self.text
        if not original or original not in current:
            return False
        self.text = current.replace(original, placeholder, 1)
        return True
