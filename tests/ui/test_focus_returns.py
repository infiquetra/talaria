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
