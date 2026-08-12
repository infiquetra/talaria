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


# B1 removed the caret status row (docs/plans/2026-08-11-v0-3-unit-b1-caret-status-row.md
# KTD1). The two tests that asserted its content — tab-into-transcript and
# F1-jump naming — are deleted with this note rather than silently vanishing,
# so a reader knows the absence is deliberate and where the replacement is
# asserted (AE1/AE2 in tests/ui/test_b1_discard_notice.py).

@pytest.mark.asyncio
async def test_f1_reaches_a_button_backed_card_past_agent_rows() -> None:
    """A4 AE1: F1 is removed — the card owns focus when the composer is empty,
    and the jump has no job. This test keeps the composer non-empty (the mid-word
    guard) and verifies that pressing F1 now does nothing, while the underlying
    ``focus_first_unanswered`` helper still reaches past the rows when called
    directly (the deliberate-steal path is preserved, just not on F1).

    Two agent rows are mounted first — the variable-tab-distance case.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        await pilot.press("h", "i")
        await pilot.pause()
        assert app.composer.text == "hi"
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
        assert app.screen.focused is app.composer.text_area, (
            "mid-word guard keeps composer focused"
        )

        await pilot.press("f1")
        await pilot.pause()
        # A4: F1 no longer bound — focus stays on the composer.
        assert app.screen.focused is app.composer.text_area
        # The helper still reaches past the rows when invoked directly.
        assert app.prompts.focus_first_unanswered()
        await pilot.pause()
        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        focused: Widget | None = app.screen.focused
        assert focused is card.query_one("#choice-0", Button)
        await app.shutdown_sources()


# B1 removed the second caret-slot assertion (see note above).

@pytest.mark.asyncio
async def test_f1_jumps_even_while_the_composer_holds_text() -> None:
    """A4: F1 no longer jumps — the deliberate-steal path is now via
    ``focus_first_unanswered`` directly. Typed text is still never touched by
    a focus move, whether via the helper or via the composer guard."""
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
        # A4: F1 does not move focus; helper does.
        assert app.screen.focused is app.composer.text_area
        assert app.prompts.focus_first_unanswered()
        await pilot.pause()
        card = app.prompts.card_for("c-1")
        assert card is not None
        focused: Widget | None = app.screen.focused
        assert focused is card.query_one("#answer", Input)
        assert app.composer.text == "hi"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f1_with_nothing_outstanding_is_a_no_op() -> None:
    """A4: F1 is not bound — pressing it with nothing outstanding leaves the
    caret and the composer's text exactly where they were, with no notice.
    The former B3 notice (JUMP_NOTHING_OUTSTANDING) is gone with the binding.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        await pilot.press("h", "i")
        await pilot.pause()
        before = app.screen.focused
        before_rows = len(app.state.transcript)
        before_notice = app.composer.notice

        await pilot.press("f1")
        await pilot.pause()

        assert app.screen.focused is before
        assert app.composer.text == "hi"
        # No binding, no notice.
        assert app.composer.notice == before_notice
        assert len(app.state.transcript) == before_rows, "a no-op must not append a transcript row"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f1_is_refused_while_a_modal_picker_is_open() -> None:
    """A4: F1 is not bound — pressing it while a modal picker is open does
    nothing to either screen's focus and shows no jump notice. The helper's
    background-focus hazard is moot without the binding.
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

        before_notice = app.composer.notice
        await pilot.press("f1")
        await pilot.pause()

        assert isinstance(app.screen, PickerDialog), "F1 must not dismiss the picker"
        assert app.screen.focused is modal_focus_before, "the modal's own focus must not move"
        assert base_screen.focused is focus_before_picker, (
            "the background card must not gain focus while it cannot be seen or used"
        )
        assert app.composer.notice == before_notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_answering_via_the_f1_jumped_to_control_hands_the_caret_back() -> None:
    """A4: the helper still lands on the control, and answering via that
    control hands the caret back through ``CaretReleased``, same as via tab or
    click. The entry is via the helper directly, not via F1.
    """
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
        assert app.prompts.focus_first_unanswered()
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


@pytest.mark.asyncio
async def test_a1_approval_card_takes_focus_on_mount_when_composer_empty() -> None:
    """A1/A2 AE1: an approval card takes focus when it mounts, without a jump key.

    With the composer empty, a gateway ``approval.request`` with a closed choice
    list is button-backed. After ``settle`` the first button (``#choice-0``) holds
    the caret — the same widget ``PromptCard.focus_answer`` lands on for ``F1``,
    and the same ``holds_caret`` the region uses to decide whether a second card
    may steal.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        assert app.composer.text.strip() == "", "composer must start empty for AE1"
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
        )
        await settle(app, pilot)

        card = app.prompts.card_for("approval:s1#1")
        assert card is not None
        focused: Widget | None = app.screen.focused
        assert focused is card.query_one("#choice-0", Button)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a1_second_card_does_not_steal_focus_from_the_first() -> None:
    """A1/A2 AE4: a second card does not steal focus from the first.

    Mount a first approval, let it auto-focus (AE1). Without moving focus,
    feed a second ``approval.request``. After ``settle`` the first card still
    holds the caret and the second does not, regardless of the uncorrelated-
    approval rebuild that turns both cards into ``deny-all`` once two are queued.
    """
    from talaria.ui.focus import holds_caret

    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "first",
                    "command": "cmd1",
                    "choices": ["once", "deny"],
                },
            ),
        )
        await settle(app, pilot)

        first = app.prompts.card_for("approval:s1#1")
        assert first is not None
        assert app.screen.focused is first.query_one("#choice-0", Button)

        feed(
            app,
            event(
                "approval.request",
                {
                    "description": "second",
                    "command": "cmd2",
                    "choices": ["once", "deny"],
                },
            ),
        )
        await settle(app, pilot)

        first_after = app.prompts.card_for("approval:s1#1")
        second = app.prompts.card_for("approval:s1#2")
        assert first_after is not None and second is not None
        # Both cards are now deny-all (uncorrelated-approval rule), so the
        # first card's control is ``#deny-all``, not ``#choice-0``.
        assert app.screen.focused is first_after.query_one("#deny-all", Button)
        assert holds_caret(first_after)
        assert not holds_caret(second)
        await app.shutdown_sources()
