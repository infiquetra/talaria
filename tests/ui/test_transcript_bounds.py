"""R38's bounded mount, scroll anchors, reflow, and content completeness (AE5).

The pairing matters more than either test alone. A pane that never grows is
trivial to write — it just throws content away — so every bound assertion here
is paired with a reachability assertion taken from the same projection the
agent's ``read_terminal`` is served from. Bounded *and* complete is the claim;
either half on its own is not.
"""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path
from typing import Any

import pytest
from textual import events
from textual.widgets import Static

from talaria.domain.projection import terminal_read, transcript_view
from talaria.replay.gate import content_is_complete
from talaria.status.runner import StatusRunner
from talaria.ui.app import ALREADY_FOLLOWING_BOTTOM
from tests.ui.conftest import (
    RecordingDispatcher,
    event,
    feed,
    live_app,
    paused_app,
    records,
    settle,
    streaming_turn,
)

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
async def test_f5_and_end_confirm_a_repeat_follow_without_losing_the_anchor() -> None:
    """B3 (AE3): re-following the newest line at the bottom of a paused
    replay is a legitimate no-op, and the old silence there was exactly
    charter E2's ambiguity. The first press — the one that visibly scrolls —
    stays silent; the repeat press, which changes nothing, says so. ``end``
    shares the rule through the same method (KTD2), so the two keys cannot
    drift.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        # Not following: the press scrolls the view, so no notice is owed.
        await pilot.press("pageup")
        await pilot.pause()
        assert app.transcript.follow is False
        await pilot.press("f5")
        await pilot.pause()
        assert app.transcript.follow is True
        assert app.composer.notice == "", "a visible scroll needs no confirmation"

        # Already following: both keys say so, and follow stays true.
        await pilot.press("end")
        await pilot.pause()
        assert app.transcript.follow is True
        assert ALREADY_FOLLOWING_BOTTOM in app.composer.notice
        await pilot.press("f5")
        await pilot.pause()
        assert app.transcript.follow is True
        assert ALREADY_FOLLOWING_BOTTOM in app.composer.notice
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
async def test_identity_anchor_survives_append_status_growth_and_resize(
    stress_frames: list[dict[str, Any]],
    tmp_path: Path,
) -> None:
    app, controls = paused_app(stress_frames, mount_cap=200)
    async with app.run_test(size=(80, 20)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript
        assert pane.max_scroll_y > 2

        pane.scroll_to(y=max(1, pane.max_scroll_y // 2), animate=False, immediate=True)
        await pilot.pause()
        pane.hold_anchor()
        await pilot.pause()
        held = pane.capture_reading_anchor()
        assert held is not None

        for seq, frame in enumerate(
            streaming_turn(["new output one\n", "new output two\n"]),
            start=10_000,
        ):
            feed(app, frame, seq=seq)
        await settle(app, pilot)
        assert pane.follow is False
        assert pane.capture_reading_anchor() == held
        assert pane.scroll_offset.y < pane.max_scroll_y

        app.status_runner = StatusRunner(
            argv=(sys.executable, "-c", "print('branch: main\\ntests: passing')"),
            launch_cwd=tmp_path,
            parent_env={},
        )
        status_anchor = pane.capture_reading_anchor()
        result = await app.status_tick()
        await pilot.pause()
        await pilot.pause()
        assert result is not None and result.outcome == "ok"
        assert pane.capture_reading_anchor() == status_anchor

        resize_anchor = pane.capture_reading_anchor()
        await pilot.resize_terminal(62, 24)
        await pilot.pause()
        await pilot.pause()
        assert pane.follow is False
        assert pane.capture_reading_anchor() == resize_anchor
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_preview_and_inspector_reflow_restore_the_reading_anchor(
    stress_frames: list[dict[str, Any]],
) -> None:
    app, controls = paused_app(stress_frames, mount_cap=200)
    async with app.run_test(size=(132, 24)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript
        pane.scroll_to(y=max(1, pane.max_scroll_y // 2), animate=False, immediate=True)
        await pilot.pause()
        pane.hold_anchor()
        await pilot.pause()
        held = pane.capture_reading_anchor()
        assert held is not None

        await app.open_theme_picker()
        await pilot.pause()
        app.palette.move_theme_selection(1)
        await pilot.pause()
        await pilot.pause()
        assert pane.capture_reading_anchor() == held

        await app.palette.cancel_theme_selection()
        await pilot.pause()
        await pilot.pause()
        assert pane.capture_reading_anchor() == held

        app.action_toggle_inspector()
        await pilot.pause()
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed is True
        assert pane.capture_reading_anchor() == held
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_pointer_scroll_unpins_before_later_chrome_reflow(
    stress_frames: list[dict[str, Any]],
) -> None:
    """A wheel event is consumed by the scroll pane before it can bubble to
    the app. The pane must unpin there so later chrome changes restore the
    reader's position rather than treating it as a follow-bottom position.
    """
    app, controls = paused_app(stress_frames, mount_cap=200)
    async with app.run_test(size=(132, 36)) as pilot:
        await _drain(app, pilot, controls)
        pane = app.transcript
        assert pane.follow is True
        for _ in range(5):
            pane.post_message(
                events.MouseScrollUp(
                    pane,
                    x=10,
                    y=10,
                    delta_x=0,
                    delta_y=-1,
                    button=0,
                    shift=False,
                    meta=False,
                    ctrl=False,
                    screen_x=10,
                    screen_y=10,
                )
            )
        await pilot.pause()
        assert pane.follow is False
        held = pane.capture_reading_anchor()
        assert held is not None

        app.action_toggle_inspector()
        await pilot.pause()
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed is True
        assert pane.capture_reading_anchor() == held
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_only_the_exact_bottom_follows_the_next_append() -> None:
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(70, 18)) as pilot:
        seq = 100
        for turn in range(20):
            for frame in streaming_turn([f"turn {turn} line one\nline two\n"]):
                feed(app, frame, seq=seq)
                seq += 1
        await settle(app, pilot)
        pane = app.transcript

        pane.follow_bottom()
        await pilot.pause()
        assert pane.scroll_offset.y == pane.max_scroll_y
        for frame in streaming_turn(["pinned append\n"]):
            feed(app, frame, seq=seq)
            seq += 1
        await settle(app, pilot)
        assert pane.follow is True
        assert pane.scroll_offset.y == pane.max_scroll_y

        pane.scroll_to(y=max(0, pane.max_scroll_y - 1), animate=False, immediate=True)
        await pilot.pause()
        pane.hold_anchor()
        await pilot.pause()
        one_row_away = pane.capture_reading_anchor()
        assert one_row_away is not None
        for frame in streaming_turn(["unpinned append\n"]):
            feed(app, frame, seq=seq)
            seq += 1
        await settle(app, pilot)
        assert pane.follow is False
        assert pane.capture_reading_anchor() == one_row_away
        assert pane.scroll_offset.y < pane.max_scroll_y
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

    The test above checks the symptom on one sequence. This checks the rule
    that makes every such sequence safe, so a future refactor that
    reintroduces the bug by another route still fails: content the pane
    treats as settled (mounted into ``_entries``, keyed by a committed
    entry id) must be content the domain has actually committed, because
    committed entries are the only ones that can never move.

    U4 restates the mechanism this pins: v0.1 tracked settledness as a
    single line-index floor (``_stable``) compared against
    ``committed_lines``; U4 tracks it per entry id instead
    (``TranscriptPane._entries`` is populated only from
    ``EntryScopedView.entries``, KTD6's *committed* surface — a live,
    uncommitted stream lives only in ``TranscriptPane._tails``). No frame
    in this fixture ever commits (there is no ``message.complete``), so
    the invariant this pins is that growing streamed content never leaks
    into ``_entries`` while it stays provisional.
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
            assert not app.transcript._entries, (
                "no entry may be treated as committed before the domain commits it"
            )
            seen += 1
        # A run in which the streaming block never grew would satisfy the
        # assertion vacuously, so prove the block was really there.
        assert seen == len(frames)
        assert transcript_view(app.state).committed_lines == 0
        assert transcript_view(app.state).total_lines == 3
        assert app.transcript._tails["assistant"] is not None, "the block was really there"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_projection_and_the_domain_transcript_agree_at_every_pause_point(
    stress_frames: list[dict[str, Any]],
) -> None:
    """KTD14's zero-content-loss clause, checked repeatedly rather than once.

    Previous shape resumed and paused a replay up to twelve times, breaking
    early once ``replay_complete`` was set, then asserted ``checked >= 3``.
    That asserted how fast the runner is: on a fast Linux runner the replay
    drained in two cycles and the run failed with ``assert 2 >= 3``. The
    guarantee the test exists to prove — ``content_is_complete`` at every
    sampled point — held on both samples.

    This shape controls the sample count: it ingests the stress corpus in
    deterministic batches, renders after each, and asserts completeness each
    time. The number of checks is a value the test controls (``len(batches)``),
    not one it observes via wall-clock scheduling. Rejected: feeding more
    frames (mitigation, still probabilistic, costs wall-clock) and adding a
    stepping API to ``ReplayControls`` (adds production surface purely for a
    test — see ``talaria/replay/controls.py:87`` — when the existing
    ``ingest`` path already provides a deterministic drive with no new API).
    """
    app, _ = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)):
        all_records = records(stress_frames)
        # A sixth-of-the-corpus batch guarantees at least three checks (the
        # original guard) for any corpus that triggers the mount cap. For the
        # 321-frame stress fixture the batch is 53 frames, which yields seven
        # checks rather than six — integer division floors, so the remainder
        # gets a short final batch. Either way the count is ceil(n/batch_size),
        # a value this test computes, not one it observes: wall-clock
        # independent and impossible to fail because of runner speed.
        batch_size = max(1, len(all_records) // 6)
        checked = 0
        for start in range(0, len(all_records), batch_size):
            for rec in all_records[start : start + batch_size]:
                app.ingest(rec)
            await app.render_snapshot()
            assert content_is_complete(app.state, transcript_view(app.state))
            checked += 1
        assert checked >= 3
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_two_renders_can_never_reconcile_the_pane_at_the_same_time(
    stress_frames: list[dict[str, Any]],
) -> None:
    """The mechanism behind this file's long-standing intermittent failure.

    ``render_snapshot`` has two kinds of caller. The coalescing timer runs on
    Textual's message pump and never re-enters itself; forced flushes —
    ``TalariaApp.drain`` and the gate's checkpoints — run on whatever task called
    them. The two are not serialized with each other, and ``TranscriptPane.apply``
    is a read-modify-write over its own window bookkeeping spanning several
    awaits. A second pass that starts while the first is inside ``apply``, over a
    *different* projection, leaves the pane holding a window the projection does
    not have.

    Measured before the fix as a **one-line skew** — ``'line 38.3' != 'line
    38.4'`` at index 30 — in the mounted-widget test above: 2 failures in 12 runs
    of this file plus ``tests/replay``, and only under load. With
    ``render_snapshot`` serialized, 12 of 12 clean.

    Three details make this deterministic where the original failure was
    probabilistic, and each of them was a version of this test that passed
    against the unfixed code:

    * The second render starts while the first is *parked inside* ``apply``, not
      merely at the same time. Two renders launched together never overlap: the
      first stores its snapshot before it awaits, so the second finds nothing
      changed and returns.
    * **Both** renders need real mounting work. ``apply`` over a view the pane
      already holds returns without ever yielding, so the first render finishes
      before the second can enter. Hence a batch of frames folded before each.
    * The state advances between them, so the two renders carry different views —
      which is what the interleaved window bookkeeping actually corrupts.

    Measured against the unfixed code: overlap depth 2. With the lock: 1.
    """
    app, controls = paused_app(stress_frames, mount_cap=SMALL_CAP)
    async with app.run_test(size=(80, 24)) as pilot:
        await _drain(app, pilot, controls)

        pane = app.transcript
        original = pane.apply
        depth = 0
        peak = 0
        entered = asyncio.Event()

        async def counting_apply(view: Any, entries: Any) -> None:
            nonlocal depth, peak
            depth += 1
            peak = max(peak, depth)
            entered.set()
            try:
                await asyncio.sleep(0)
                await original(view, entries)
            finally:
                depth -= 1

        pane.apply = counting_apply  # type: ignore[method-assign]
        try:
            for record in records(streaming_turn([f"batch A {i}\n" for i in range(30)])):
                app.ingest(record)
            app.snapshot = None
            first = asyncio.create_task(app.render_snapshot())
            await entered.wait()

            for record in records(streaming_turn([f"batch B {i}\n" for i in range(30)])):
                app.ingest(record)
            second = asyncio.create_task(app.render_snapshot())
            await asyncio.gather(first, second)
        finally:
            pane.apply = original  # type: ignore[method-assign]

        assert peak >= 1, "the harness never reached apply, so it proves nothing"
        assert peak == 1, "two renders reconciled the pane concurrently"

        view = transcript_view(app.state)
        assert len(pane.rendered_lines) + pane.condensed_count == view.total_lines
        assert pane.rendered_lines == view.lines[pane.condensed_count :]
        await app.shutdown_sources()
