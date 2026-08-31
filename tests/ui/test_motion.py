"""The restart-scoped reduced-motion policy changes decoration, not state."""

from __future__ import annotations

import pytest

from talaria.domain.projection import PromptView
from talaria.ui.motion import REDUCED_MOTION, STANDARD_MOTION, MotionPolicy
from talaria.ui.prompts import activity_line
from tests.ui.conftest import event, paused_app, screen_text


def test_reduced_motion_freezes_every_nonessential_progress_frame() -> None:
    frames = ("[-]", "[\\]", "[|]", "[/]")

    assert tuple(STANDARD_MOTION.progress_frame(frames, index) for index in range(4)) == frames
    assert {
        REDUCED_MOTION.progress_frame(frames, index) for index in range(20)
    } == {"[..]"}
    assert REDUCED_MOTION.progress_frame((), 0) == "[..]"
    assert activity_line("streaming", PromptView(), REDUCED_MOTION) == "[..] working"
    assert activity_line("streaming", PromptView(), STANDARD_MOTION) == "working…"


@pytest.mark.parametrize("requested", [False, True])
def test_reduced_motion_makes_every_routed_scroll_immediate(requested: bool) -> None:
    motion = REDUCED_MOTION.scroll(animate=requested, duration=0.25)
    assert motion.animate is False
    assert motion.duration == 0.0


def test_ordinary_motion_retains_the_callers_existing_scroll_choice() -> None:
    animated = STANDARD_MOTION.scroll(animate=True, duration=0.25)
    immediate = STANDARD_MOTION.scroll(animate=False)

    assert animated.animate is True and animated.duration == 0.25
    assert immediate.animate is False and immediate.duration is None


def test_motion_policy_is_one_immutable_restart_scoped_value() -> None:
    with pytest.raises(AttributeError):
        STANDARD_MOTION.reduced = True  # type: ignore[misc]
    assert MotionPolicy(reduced=True) == REDUCED_MOTION


@pytest.mark.asyncio
async def test_one_reduced_motion_policy_reaches_every_live_widget() -> None:
    notice = "ui.reduced_motion must be a boolean; using false"
    app, _ = paused_app(
        [event("gateway.ready", {})],
        reduced_motion=True,
        startup_notices=(notice,),
    )

    assert any(notice in entry.text for entry in app.state.transcript)
    async with app.run_test(size=(80, 24)) as pilot:
        await app.render_snapshot()
        await pilot.pause()

        assert app.motion is app.transcript.motion
        assert app.motion is app.prompts.motion
        assert app.motion.reduced is True
        assert notice in screen_text(app)
        await app.shutdown_sources()
