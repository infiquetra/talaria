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
from tests.ui.conftest import event, paused_app, records, streaming_turn

SMALL_CAP = 40

#: The bound, transient included — line widgets plus the one condensed block.
#:
#: This was ``2 * SMALL_CAP + 1``, because ``apply`` mounted the new batch and
#: *then* trimmed to the cap, awaiting in between: a sample taken mid-reconcile
#: legitimately saw the pre-existing widgets and the whole new batch at once.
#: That is no longer a legitimate transient. The pane condenses from the top
#: before mounting, so the widget count never exceeds the cap at any instant,
#: and the gate measured the difference — 667 widgets against KTD14's ceiling of
#: 600 under the old order, 501 under this one. The tight bound is asserted
#: against ``peak_mounted``, which is sampled at the moment of maximum mount, so
#: passing it is a claim about the pane rather than about where the sample sits.
TRANSIENT_CAP = SMALL_CAP + 1


async def _drain(app: Any, pilot: Any, controls: Any) -> None:
    """Replay to the end *and* leave the pane caught up with the projection.

    "The stream stopped" and "the screen is current" are different states. The
    renderer flushes on a 50ms coalescing boundary, and that timer may not fire
    again after the last frame lands, so `drain` alone leaves the pane one flush
    behind — legitimately, by design. Every assertion about what the pane holds
    has to be taken after a flush has actually run, which is why the gate's
    settled checkpoint forces one too. Without this the accounting assertions
    below fail roughly one run in three, and only under whole-suite load.
    """
    controls.resume()
    await app.drain(timeout=60.0)
    await pilot.pause()
    await app.render_snapshot()
    await pilot.pause()


@pytest.mark.asyncio
async def test_mounted_widgets_stay_under_the_cap_while_content_stays_reachable(
    stress_frames: list[dict[str, Any]],
) -> None:
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript

        # Bounded, settled and transient alike — the pane condenses before it
        # mounts, so there is no instant at which the cap is exceeded.
        assert pane.mounted_count <= SMALL_CAP + 1
        assert pane.peak_mounted <= TRANSIENT_CAP
        assert pane.condensed_count > 0, "the corpus never exceeded the cap"

        # Complete: everything the domain committed is still served.
        view = transcript_view(app.state)
        assert view.total_lines > SMALL_CAP * 4
        assert content_is_complete(app.state, view)

        # The accounting closes: every line is either mounted or condensed,
        # exactly once. This is the invariant that a cumulative eviction tally
        # silently violated — it reached 7,493 on the 50,000-delta corpus for a
        # transcript of 4,454 lines, so the window sat at an index the
        # projection did not have and the pane rendered a wrong slice of a
        # correct projection while every bound above still passed.
        assert pane.condensed_count <= view.total_lines
        assert len(pane.rendered_lines) + pane.condensed_count == view.total_lines
        assert pane.rendered_lines == view.lines[pane.condensed_count :]

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
            # Mid-storm, this sample can land inside a reconcile, between the
            # mount and the trim. TRANSIENT_CAP is the bound that holds there;
            # asserting SMALL_CAP + 1 here is a race that passes on a fast
            # machine and fails on a loaded CI runner, which is how this was
            # found. The settled bound is asserted after the drain below.
            assert app.transcript.mounted_count <= TRANSIENT_CAP
            assert content_is_complete(app.state, transcript_view(app.state))

        await app.drain(timeout=60.0)
        await pilot.pause()
        # Settled: no reconcile in flight, so the tight bound must hold.
        assert app.transcript.mounted_count <= SMALL_CAP + 1

        view = transcript_view(app.state)
        assert content_is_complete(app.state, view)
        # Reflow, not truncation: the narrow pass must not have clipped text.
        assert "line 39.5" in view.text
        assert app.transcript.follow is False
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_entry_committed_mid_stream_shifts_the_provisional_block_correctly() -> None:
    """The reconcile defect that failed the U5 gate, pinned at its smallest size.

    The provisional streaming block sits *after* the committed lines, so
    committing an entry while a turn is streaming pushes every provisional line
    down. The pane used to record its stable floor at the true divergence point,
    which walks into the streaming block whenever two consecutive snapshots
    happen to agree on a provisional line — and multi-line streaming text makes
    them agree constantly, since each delta only rewrites the last line. The
    floor then sat above lines that were about to move, the scan never looked at
    them again, and the pane stayed misaligned for the rest of the session:
    ``alpha beta beta gamma`` rendered against ``notice alpha beta gamma``.

    Three deltas, then one committed entry. That is the whole reproduction.
    """
    frames: list[dict[str, Any]] = [
        event("message.start", {}),
        event("message.delta", {"text": "alpha\n"}),
        event("message.delta", {"text": "beta\n"}),
        event("message.delta", {"text": "gamma"}),
        # Any frame that commits an entry while the turn is still streaming; an
        # unknown event type is the cheapest one that does.
        {"jsonrpc": "2.0", "method": "event", "params": {"type": "no.such.event"}},
    ]
    app, _ = paused_app(frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)):
        for record in records(frames):
            app.ingest(record)
            await app.render_snapshot()

        view = transcript_view(app.state)
        pane = app.transcript
        assert view.committed_lines == 1, "the unknown event should have committed one line"
        assert view.lines[1:] == ("alpha", "beta", "gamma"), "streaming block did not shift"
        # The whole claim: what is on screen is what the projection says, in
        # order, with nothing duplicated and nothing dropped.
        assert pane.rendered_lines == view.lines
        assert pane.condensed_count == 0
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_stable_floor_never_advances_into_the_provisional_block() -> None:
    """The invariant behind the fix, asserted directly rather than inferred.

    The test above checks the symptom on one sequence. This checks the rule that
    makes every such sequence safe, so a future refactor that reintroduces the
    bug by another route still fails: a line the pane treats as settled must be
    one the projection has committed, because committed lines are the only ones
    that can never move.
    """
    frames: list[dict[str, Any]] = [
        event("message.start", {}),
        event("message.delta", {"text": "one\n"}),
        event("message.delta", {"text": "two\n"}),
        event("message.delta", {"text": "three"}),
    ]
    app, _ = paused_app(frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)):
        seen = 0
        for record in records(frames):
            app.ingest(record)
            await app.render_snapshot()
            view = transcript_view(app.state)
            assert app.transcript._stable <= view.committed_lines
            seen += 1
        # A run in which the streaming block never grew would satisfy the
        # assertion vacuously, so prove the block was really there.
        assert seen == len(frames)
        assert transcript_view(app.state).committed_lines == 0
        assert transcript_view(app.state).total_lines == 3
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
