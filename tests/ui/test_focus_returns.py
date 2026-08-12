"""The caret comes back when Talaria takes a control away.

Every test here presses keys and then asks what the composer contains, rather
than asking which widget Textual reports as focused. That is deliberate: the
defect these cover was not "focus is on the wrong widget", it was "the operator
types a message and no characters appear". A test that asserts on
``app.screen.focused`` passes for a widget that holds the caret and discards
every key — which is exactly what a ``VerticalScroll`` does, and exactly what
was happening. The question a test has to ask is the operator's question.

The failure was silent in every direction. The composer still showed its
placeholder, because a composer with no text always does. The interface still
answered ``ctrl+c``, because those bindings are ``priority`` and never needed
the caret. Nothing was greyed out and nothing was on screen to say the app had
stopped listening — the only reliable way out was to quit and relaunch.
"""

from __future__ import annotations

import pytest
from textual.widget import Widget
from textual.widgets import Button, Input

from talaria.domain.models_catalog import ModelProvider, ProviderCatalog
from talaria.ui.app import JUMP_BLOCKED_BY_MODAL, JUMP_NOTHING_OUTSTANDING
from talaria.ui.dialog import PickerDialog
from tests.ui.conftest import RecordingDispatcher, event, feed, live_app, settle


def _subagent_start(index: int, name: str) -> dict[str, object]:
    return event(
        "subagent.start",
        {"subagent_id": f"a{index}", "goal": name, "depth": 1, "task_index": index},
    )


@pytest.mark.asyncio
async def test_answering_a_typed_prompt_hands_the_caret_back() -> None:
    """The keyboard-only path, and the one an operator hits first.

    A clarify prompt takes the caret on purpose — a blocking question has a real
    claim on it. What it must not do is keep it after it is answered, and the
    answer is submitted from inside the control that is about to be removed, so
    there is no operator action in between to mask the loss.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        await settle(app, pilot)
        assert app.prompts.card_for("c-1") is not None, "no prompt, so nothing to lose"

        await pilot.press("m", "a", "i", "n", "enter")
        await settle(app, pilot)
        await settle(app, pilot)
        assert list(app.prompts.card_ids) == [], "the card is still up; the caret is still its own"

        await pilot.press("h", "e", "l", "l", "o")
        await pilot.pause()
        assert app.composer.text == "hello"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_answering_an_approval_by_clicking_hands_the_caret_back() -> None:
    """The mouse path. Textual hands the caret to the enclosing ``PromptRegion``
    here, which is focusable so that arrow keys scroll it and which silently
    drops every printable key it is given."""
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        feed(app, event("message.start", {}))
        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "delete the build directory",
                    "command": "rm -rf build",
                    "choices": ["once", "session", "deny"],
                },
            ),
            seq=101,
        )
        await settle(app, pilot)

        await pilot.click("#choice-0")
        await settle(app, pilot)
        await settle(app, pilot)
        assert list(app.prompts.card_ids) == []

        await pilot.press("o", "k")
        await pilot.pause()
        assert app.composer.text == "ok"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_child_finishing_hands_back_the_caret_its_row_was_holding() -> None:
    """The case Textual does not cover at all.

    A finished child's row is not removed — it is re-pointed in place and its
    ``can_focus`` flips to ``False``. Textual moves the caret off a widget that
    was *removed*; a widget that merely stopped being focusable keeps it, and
    then sits outside the focus chain eating keys. So this one cannot be
    delegated to the framework, and it fires without the operator doing
    anything: the child finishes on its own schedule.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        feed(app, _subagent_start(0, "indexer"), seq=101)
        feed(app, _subagent_start(1, "reviewer"), seq=102)
        await settle(app, pilot)

        row = app.agents.row_for("a0")
        assert row is not None and row.interruptible, "nothing to focus if it is not interruptible"
        row.focus()
        await pilot.pause()

        feed(app, event("subagent.complete", {"subagent_id": "a0", "status": "completed"}), seq=200)
        await settle(app, pilot)
        assert not row.interruptible, "the child did not finish; the caret was never at risk"

        await pilot.press("d", "o", "n", "e")
        await pilot.pause()
        assert app.composer.text == "done"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_collapsing_the_rows_hands_the_caret_back() -> None:
    """f2 removes every row at once. An operator who loses the caret to their
    own keystroke is the least likely to suspect the interface of it."""
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        feed(app, _subagent_start(0, "indexer"), seq=101)
        await settle(app, pilot)

        row = app.agents.row_for("a0")
        assert row is not None
        row.focus()
        await pilot.pause()

        await pilot.press("f2")
        await settle(app, pilot)
        assert app.agents.row_texts == (), "nothing collapsed, so no row was taken away"

        await pilot.press("h", "i")
        await pilot.pause()
        assert app.composer.text == "hi"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_deliberate_focus_move_is_left_alone() -> None:
    """The other half of the rule, and the reason it is event-driven.

    The caret is returned when Talaria *takes a control away*, not on a timer
    and not on every render. An operator who tabs to the transcript to scroll it
    with the arrow keys has moved the caret on purpose, and a fix that clamped
    focus in the render pass would drag it back roughly twenty times a second —
    making the transcript unusable to trade for a bug in the prompt region.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        reply = event("message.delta", {"text": "a reply worth scrolling back through\n"})
        feed(app, reply, seq=101)
        await settle(app, pilot)
        assert app.screen.focused is app.composer.text_area

        await pilot.press("tab")
        await pilot.pause()
        moved = app.screen.focused
        assert moved is not app.composer.text_area, "tab moved nothing; this proves nothing"

        # Several renders, which is what a timer-based clamp would have ridden.
        await settle(app, pilot)
        await settle(app, pilot)
        assert app.screen.focused is moved
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_tabbing_into_the_transcript_names_it_in_the_caret_slot() -> None:
    """U3's dedicated caret slot (R5, KTD5). The transcript pane gives no
    visual sign of holding the caret on its own — this is the one on-screen
    word that says so, and it clears again the moment the caret comes home.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        reply = event("message.delta", {"text": "a reply worth scrolling back through\n"})
        feed(app, reply, seq=101)
        await settle(app, pilot)
        assert app.screen.focused is app.composer.text_area
        assert app.status_region.caret_text == ""

        await pilot.press("tab")
        await pilot.pause()
        assert app.screen.focused is not app.composer.text_area
        assert app.status_region.caret_text == "caret: transcript"

        await pilot.press("shift+tab")
        await pilot.pause()
        assert app.screen.focused is app.composer.text_area
        assert app.status_region.caret_text == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f1_reaches_a_button_backed_card_past_agent_rows() -> None:
    """R1: one press reaches the control regardless of how much sits above it.

    Two agent rows are mounted first — the variable-tab-distance case — so
    the approval's control is not the first focusable widget on screen, which
    is exactly the arrangement a tab-order-only fix would still get wrong.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        feed(app, _subagent_start(0, "indexer"), seq=101)
        feed(app, _subagent_start(1, "reviewer"), seq=102)
        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "delete the build directory",
                    "command": "rm -rf build",
                    "choices": ["once", "deny"],
                },
            ),
            seq=103,
        )
        await settle(app, pilot)
        assert app.screen.focused is app.composer.text_area, "mount-time auto-focus is out of scope"

        await pilot.press("f1")
        await pilot.pause()

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        # Read once into an explicitly typed local: the assert above narrowed
        # ``app.screen.focused`` to the composer's text area, and mypy keeps
        # that narrowing across the keypress the test exists to make.
        focused: Widget | None = app.screen.focused
        assert focused is card.query_one("#choice-0", Button)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f1_jump_names_the_prompts_region_in_the_caret_slot() -> None:
    """U3's caret slot, driven by U1's jump key. Landing on a card's control
    is exactly the case the slot exists for: a button holding the caret
    looks like any other unfocused button unless something says so.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        feed(app, event("message.start", {}))
        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "delete the build directory",
                    "command": "rm -rf build",
                    "choices": ["once", "deny"],
                },
            ),
            seq=101,
        )
        await settle(app, pilot)
        assert app.status_region.caret_text == ""

        await pilot.press("f1")
        await pilot.pause()

        assert app.status_region.caret_text == "caret: prompts"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f1_jumps_even_while_the_composer_holds_text() -> None:
    """KTD1: the jump is deliberate and may steal the caret mid-word — the
    typed text itself is never touched by the act of jumping away from it."""
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        await pilot.press("h", "i")
        await pilot.pause()
        assert app.composer.text == "hi"

        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        await settle(app, pilot)
        assert app.screen.focused is app.composer.text_area, "composer text guards mount-time focus"

        await pilot.press("f1")
        await pilot.pause()

        card = app.prompts.card_for("c-1")
        assert card is not None
        # Read once into an explicitly typed local: the assert above narrowed
        # ``app.screen.focused`` to the composer's text area, and mypy keeps
        # that narrowing across the keypress the test exists to make.
        focused: Widget | None = app.screen.focused
        assert focused is card.query_one("#answer", Input)
        assert app.composer.text == "hi"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f1_with_nothing_outstanding_is_a_no_op() -> None:
    """No card, nothing to jump to — the caret and the composer's text are
    both left exactly where the operator put them.

    B3 (AE1): the no-op used to be indistinguishable from a dead key, so it
    now says so — one notice naming that no prompt is waiting, and no
    transcript row, because a no-op is not part of the session's story.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        await pilot.press("h", "i")
        await pilot.pause()
        before = app.screen.focused
        before_rows = len(app.state.transcript)

        await pilot.press("f1")
        await pilot.pause()

        assert app.screen.focused is before
        assert app.composer.text == "hi"
        assert JUMP_NOTHING_OUTSTANDING in app.composer.notice
        assert len(app.state.transcript) == before_rows, "a no-op must not append a transcript row"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f1_is_refused_while_a_modal_picker_is_open() -> None:
    """CR1 finding 2: ``Widget.focus()`` (which ``focus_answer()`` ends in)
    calls ``widget.screen.set_focus(...)`` — the card's OWN background
    screen, never the active top-of-stack modal. With a picker open, F1 used
    to change a background button's has-focus state with no visible or
    functional effect at all: the modal kept receiving every keystroke, and
    the operator had no way to tell "worked" from "did nothing" (AE11). F1
    is refused instead, with a notice, and neither screen's focus moves.
    """
    app = live_app(RecordingDispatcher())
    app.model_catalog = ProviderCatalog(
        providers=(
            ModelProvider(
                slug="anthropic",
                name="Anthropic",
                models=("opus",),
                authenticated=True,
                is_current=True,
            ),
        ),
        current_provider="anthropic",
        current_model="opus",
    )
    async with app.run_test() as pilot:
        # Typed first, so mount-time auto-focus does not claim the card
        # itself and leave nothing for this test's own jump to move — see
        # ``test_a_focused_card_is_visually_distinct``'s docstring for the
        # same convention.
        await pilot.press("h", "i")
        await pilot.pause()
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        await settle(app, pilot)
        base_screen = app.screen_stack[0]
        focus_before_picker = base_screen.focused
        assert focus_before_picker is app.composer.text_area

        await app.open_picker("models")
        await pilot.pause()
        assert isinstance(app.screen, PickerDialog)
        modal_focus_before = app.screen.focused

        await pilot.press("f1")
        await pilot.pause()

        assert isinstance(app.screen, PickerDialog), "F1 must not dismiss the picker"
        assert app.screen.focused is modal_focus_before, "the modal's own focus must not move"
        assert base_screen.focused is focus_before_picker, (
            "the background card must not gain focus while it cannot be seen or used"
        )
        assert JUMP_BLOCKED_BY_MODAL in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_answering_via_the_f1_jumped_to_control_hands_the_caret_back() -> None:
    """The jump is a way in, not a new way to lose the caret — pressing the
    control it landed on releases the caret through the existing
    ``CaretReleased`` path, same as answering by tab or by click."""
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        feed(app, event("message.start", {}))
        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "delete the build directory",
                    "command": "rm -rf build",
                    "choices": ["once", "deny"],
                },
            ),
            seq=101,
        )
        await settle(app, pilot)

        await pilot.press("f1")
        await pilot.pause()
        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        assert app.screen.focused is card.query_one("#choice-0", Button)

        await pilot.press("enter")
        await settle(app, pilot)
        await settle(app, pilot)
        assert list(app.prompts.card_ids) == []

        await pilot.press("d", "o", "n", "e")
        await pilot.pause()
        assert app.composer.text == "done"
        await app.shutdown_sources()
