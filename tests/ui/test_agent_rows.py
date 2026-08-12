"""Sub-agent rows during a live parent stream (R14, R16, KTD8).

R14's requirement is specifically that the rows are visible *while the parent is
still streaming* — a monitor that only appears once the turn ends tells the
operator nothing at the moment they need it. So every assertion below is taken
mid-turn, with the parent's ``message.complete`` deliberately withheld.
"""

from __future__ import annotations

import pytest

from talaria.ui.agents import format_row
from talaria.ui.app import AGENTS_NOTHING_TO_TOGGLE
from tests.ui.conftest import event, paused_app


def _mid_turn_frames() -> list[dict[str, object]]:
    frames: list[dict[str, object]] = [
        event("gateway.ready", {}),
        event("message.start", {}),
        event("message.delta", {"text": "parent is still talking "}),
    ]
    for index, name in enumerate(("indexer", "reviewer", "searcher")):
        frames.append(
            event(
                "subagent.start",
                {"subagent_id": f"a{index}", "goal": name, "depth": 1, "task_index": index},
            )
        )
    frames.append(event("subagent.complete", {"subagent_id": "a1", "status": "completed"}))
    frames.append(event("message.delta", {"text": "and still talking"}))
    return frames


@pytest.mark.asyncio
async def test_rows_are_visible_while_the_parent_is_still_streaming() -> None:
    app, controls = paused_app(_mid_turn_frames())
    async with app.run_test(size=(100, 30)) as pilot:
        controls.resume()
        await app.drain(timeout=30.0)
        await pilot.pause()

        assert app.state.turn == "streaming", "the parent turn ended; this is not mid-turn"
        rows = app.agents.row_texts
        assert len(rows) == 3
        assert any("indexer" in row and "running" in row for row in rows)
        assert any("reviewer" in row and "completed" in row for row in rows)
        assert app.agents.display is True
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_count_survives_collapsing(
) -> None:
    """R16: collapsed must not mean gone — the count stays on screen."""
    app, controls = paused_app(_mid_turn_frames())
    async with app.run_test(size=(100, 30)) as pilot:
        controls.resume()
        await app.drain(timeout=30.0)
        await pilot.pause()

        collapsed = await app.agents.toggle_collapsed()
        await pilot.pause()
        assert collapsed is True
        assert app.agents.row_texts == ()
        header = app.agents.header_text
        assert "2 active" in header and "1 finished" in header
        assert app.agents.display is True

        await app.agents.toggle_collapsed()
        await pilot.pause()
        assert len(app.agents.row_texts) == 3
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_terminal_status_is_not_clobbered_by_a_late_event() -> None:
    """KTD8's precedence, observed through the rendered row rather than the reducer."""
    frames = _mid_turn_frames()
    frames.append(event("subagent.start", {"subagent_id": "a1", "goal": "reviewer"}))
    app, controls = paused_app(frames)
    async with app.run_test(size=(100, 30)) as pilot:
        controls.resume()
        await app.drain(timeout=30.0)
        await pilot.pause()
        reviewer = [row for row in app.agents.row_texts if "reviewer" in row]
        assert len(reviewer) == 1
        assert "completed" in reviewer[0]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_region_hides_itself_when_there_are_no_sub_agents() -> None:
    app, controls = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        controls.resume()
        await app.drain(timeout=30.0)
        await pilot.pause()
        assert app.agents.row_texts == ()
        assert app.agents.display is False
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f2_with_no_sub_agents_says_so_and_the_region_stays_hidden() -> None:
    """B3 (AE4 half one): an empty region gives a toggle nothing to show or
    hide, so the keypress says so — the old silence was indistinguishable
    from a dead key. The flag still flips (behaviour is unchanged, per B3's
    scope discipline); only the silence is broken."""
    app, controls = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(100, 30)) as pilot:
        controls.resume()
        await app.drain(timeout=30.0)
        await pilot.pause()
        assert app.agents.row_texts == ()
        assert app.agents.display is False
        assert app.agents.collapsed is False

        await pilot.press("f2")
        await pilot.pause()

        assert AGENTS_NOTHING_TO_TOGGLE in app.composer.notice
        assert app.agents.display is False, "the region stays hidden"
        # The flip is what the notice is *about*: invisible today, and it
        # decides how the next fan-out arrives. Asserting only the notice
        # leaves an early return after it green, which is the one mutation
        # that survived this unit's review.
        assert app.agents.collapsed is True, "the flag flips even when nothing shows"

        await pilot.press("f2")
        await pilot.pause()

        assert app.agents.collapsed is False, "and flips back"
        assert app.agents.display is False
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_f2_with_rows_present_collapses_without_a_notice() -> None:
    """B3 (AE4 half two): with rows on screen the collapse is visible, so a
    notice would be noise on a release quietening the interface (B4). The
    collapse itself is the R16 behaviour the direct-call test above
    asserts."""
    app, controls = paused_app(_mid_turn_frames())
    async with app.run_test(size=(100, 30)) as pilot:
        controls.resume()
        await app.drain(timeout=30.0)
        await pilot.pause()
        rows_before = app.agents.row_texts
        assert len(rows_before) == 3

        await pilot.press("f2")
        await pilot.pause()

        assert AGENTS_NOTHING_TO_TOGGLE not in app.composer.notice, (
            "a visible collapse owes no confirmation"
        )
        assert app.agents.row_texts == ()
        assert app.agents.display is True
        header = app.agents.header_text
        assert "2 active" in header and "1 finished" in header
        await app.shutdown_sources()


def test_the_row_format_is_the_five_projection_fields_and_nothing_else() -> None:
    with_detail = format_row("a0", "indexer", "running", 12.25, "reading src/")
    without = format_row("a0", "indexer", "running", 12.25, None)
    assert with_detail == "running       12.2s  indexer — reading src/"
    assert without == "running       12.2s  indexer"
