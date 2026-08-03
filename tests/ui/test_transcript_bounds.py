"""R38's bounded mount, scroll anchors, reflow, and content completeness (AE5).

The pairing matters more than either test alone. A pane that never grows is
trivial to write — it just throws content away — so every bound assertion here
is paired with a reachability assertion taken from the same projection the
agent's ``read_terminal`` is served from. Bounded *and* complete is the claim;
either half on its own is not.
"""

from __future__ import annotations

from typing import Any

import pytest
from textual.widgets import Static

from talaria.domain.projection import terminal_read, transcript_view
from talaria.replay.gate import content_is_complete
from tests.ui.conftest import event, paused_app, streaming_turn

SMALL_CAP = 40


async def _drain(app: Any, pilot: Any, controls: Any) -> None:
    controls.resume()
    await app.drain(timeout=60.0)
    await pilot.pause()


@pytest.mark.asyncio
async def test_mounted_widgets_stay_under_the_cap_while_content_stays_reachable(
    stress_frames: list[dict[str, Any]],
) -> None:
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript

        # Bounded: the cap plus the single condensed block, never more.
        assert pane.peak_mounted <= SMALL_CAP + 1
        assert pane.mounted_count <= SMALL_CAP + 1
        assert pane.condensed_count > 0, "the corpus never exceeded the cap"

        # Complete: everything the domain committed is still served.
        view = transcript_view(app.state)
        assert view.total_lines > SMALL_CAP * 4
        assert content_is_complete(app.state, view)

        # And reachable through the same call the agent makes.
        whole = terminal_read(view, start_line=0, count=view.total_lines)
        assert whole.total_lines == view.total_lines
        assert "line 0.0" in whole.text and "line 39.5" in whole.text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_condensed_block_is_one_widget_no_matter_how_much_it_covers(
    stress_frames: list[dict[str, Any]],
) -> None:
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript
        condensed = pane.query(Static).filter(".transcript--condensed")
        assert len(condensed) == 1
        assert str(pane.condensed_count) in str(condensed.first(Static).content)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_backlog_larger_than_the_cap_is_never_mounted_in_full() -> None:
    """The transient the steady-state cap cannot see.

    A whole corpus applied in one render tick must not briefly mount every
    line. The peak counter is written inside ``apply`` before the anchor is
    restored, so a mount-then-remove implementation would show up here.
    """
    frames: list[dict[str, Any]] = [event("gateway.ready", {})]
    for turn in range(60):
        frames.extend(streaming_turn([f"bulk {turn}.{step}\n" for step in range(8)]))
    app, controls = paused_app(frames, mount_cap=SMALL_CAP, coalesce_interval=30.0)
    async with app.run_test(size=(80, 24)) as pilot:
        controls.resume()
        await app.drain(timeout=60.0)
        await pilot.pause()
        # One render tick handled the entire backlog.
        assert app.render_ticks == 1
        assert app.transcript.peak_mounted <= SMALL_CAP + 1
        assert content_is_complete(app.state, transcript_view(app.state))
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_following_the_bottom_and_holding_an_anchor_are_different_states(
    stress_frames: list[dict[str, Any]],
) -> None:
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)) as pilot:
        controls.resume()
        await pilot.pause()
        pane = app.transcript

        assert pane.follow is True
        pane.hold_anchor()
        assert pane.follow is False

        held = pane.scroll_offset.y
        # Streaming continues while the operator reads.
        for _ in range(20):
            await pilot.pause()
        assert pane.follow is False
        # The view did not jump to the end behind the reader's back.
        assert pane.scroll_offset.y <= held + 1

        pane.follow_bottom()
        await pilot.pause()
        assert pane.follow is True
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_end_and_pageup_toggle_the_anchor(stress_frames: list[dict[str, Any]]) -> None:
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)) as pilot:
        controls.resume()
        await pilot.pause()
        await pilot.press("pageup")
        await pilot.pause()
        assert app.transcript.follow is False
        await pilot.press("end")
        await pilot.pause()
        assert app.transcript.follow is True
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_resize_storm_preserves_reflow_anchors_and_content(
    stress_frames: list[dict[str, Any]],
) -> None:
    """AE5: shrink and grow repeatedly, mid-stream, and lose nothing."""
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(100, 30)) as pilot:
        controls.resume()
        await pilot.pause()
        app.transcript.hold_anchor()

        for width, height in ((40, 12), (140, 50), (30, 8), (100, 30), (52, 18)):
            await pilot.resize_terminal(width, height)
            await pilot.pause()
            assert app.transcript.mounted_count <= SMALL_CAP + 1
            assert content_is_complete(app.state, transcript_view(app.state))

        await app.drain(timeout=60.0)
        await pilot.pause()

        view = transcript_view(app.state)
        assert content_is_complete(app.state, view)
        # Reflow, not truncation: the narrow pass must not have clipped text.
        assert "line 39.5" in view.text
        assert app.transcript.follow is False
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_projection_and_the_domain_transcript_agree_at_every_pause_point(
    stress_frames: list[dict[str, Any]],
) -> None:
    """KTD14's zero-content-loss clause, checked repeatedly rather than once."""
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)) as pilot:
        checked = 0
        for _ in range(12):
            controls.resume()
            await pilot.pause()
            controls.pause()
            await pilot.pause()
            assert content_is_complete(app.state, transcript_view(app.state))
            checked += 1
            if app.replay_complete.is_set():
                break
        assert checked >= 3
        await app.shutdown_sources()
