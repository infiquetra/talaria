"""AE16 / R31: the same ordered frames, from a file and from a socket, agree.

R31 is the claim the whole replay-first ordering rests on — "replacing the frame
source with a live socket changes nothing above the transport boundary" — and it
is the easiest claim in the plan to assert weakly. Comparing two final states
would pass against a source that skipped a frame and a reducer that happened to
be idempotent over it. So the comparison here is over the **whole transition
sequence**: the domain state after every single frame, from both sources, must be
equal element by element.

**One variable is pinned rather than compared, and saying which is the honest
part.** ``FrameRecord.at`` is the *recorded* time in replay and the *receive*
time live; that difference is by design (see ``transport/source.py``), and it
propagates into ``last_observed_at`` and every sub-agent's elapsed seconds. So
the strict comparison injects a clock that replays the corpus's own recorded
times, which isolates the variable under test — the frame source — from the one
that is *supposed* to differ. A second test then runs the live path on a real
wall clock and asserts the time-independent projection is still identical, so
the pinned clock is not doing the work.

AE16 also asks that the live path meet the streaming thresholds on its own
measurements rather than inheriting replay's, which the last test does.
"""

from __future__ import annotations

import asyncio
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

from talaria.domain.normalize import normalize_frame
from talaria.domain.projection import Snapshot, project, transcript_view
from talaria.domain.state import SessionState, apply_frame
from talaria.replay.controls import ReplayControls
from talaria.replay.gate import content_is_complete
from talaria.replay.source import ReplaySource
from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential
from talaria.transport.source import FrameRecord, LiveSource
from talaria.ui.app import TalariaApp
from tests.transport.conftest import READY_FRAME, STUB_TOKEN, StubGateway, event


class FixedCredential:
    async def acquire(self) -> Credential:
        return Credential("token", STUB_TOKEN, "environment")


def corpus() -> list[dict[str, Any]]:
    """A small synthetic corpus that touches every stateful rule U3 encodes.

    Synthetic and small on purpose (R29): a recorded corpus is never committed,
    and a fixture large enough to be interesting is large enough to be a
    liability.
    """
    frames: list[dict[str, Any]] = [READY_FRAME, event("session.info", {"title": "a session"})]
    frames.append(event("message.start", {}))
    frames.extend(event("thinking.delta", {"text": f"thought {index} "}) for index in range(3))
    frames.extend(event("message.delta", {"text": f"token {index} "}) for index in range(12))
    frames.append(event("subagent.spawn_requested", {"subagent_id": "sa-1", "goal": "check"}))
    frames.append(event("subagent.start", {"subagent_id": "sa-1", "goal": "check"}))
    frames.append(event("subagent.progress", {"subagent_id": "sa-1", "text": "half way"}))
    frames.append(event("tool.start", {"name": "read_file", "context": "README.md"}))
    frames.append(event("tool.complete", {"name": "read_file", "summary": "48 lines"}))
    frames.append(event("clarify.request", {"request_id": "req-1", "question": "which one?"}))
    frames.append(event("clarify.expire", {"request_id": "req-1"}))
    frames.append(event("subagent.complete", {"subagent_id": "sa-1", "status": "completed"}))
    # A late start for a finished child, and an event for a session that is not
    # the focused one: both must be ignored identically by both sources.
    frames.append(event("subagent.start", {"subagent_id": "sa-1", "goal": "check"}))
    frames.append(event("message.delta", {"text": "elsewhere"}, session="other"))
    frames.append(event("nobody.has.ever.seen.this", {}))
    frames.append({"jsonrpc": "2.0", "id": "1", "result": {"ok": True}})
    frames.append(
        event("message.complete", {"text": "".join(f"token {i} " for i in range(12)) + "done"})
    )
    frames.append(event("status.update", {"text": "idle"}))
    return frames


def write_log(path: Path, frames: list[dict[str, Any]]) -> Path:
    lines = [
        json.dumps(
            {
                "kind": "header",
                "version": 1,
                "startedAt": "2026-08-03T09:00:00.000Z",
                "endpoint": "ws://127.0.0.1:9119/api/ws?token=%5Bredacted%5D",
            }
        )
    ]
    for index, frame in enumerate(frames):
        lines.append(
            json.dumps(
                {
                    "kind": "frame",
                    "seq": index + 1,
                    "at": f"2026-08-03T09:00:{index // 20:02d}.{(index * 37) % 1000:03d}Z",
                    "dir": "in",
                    "frame": frame,
                }
            )
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def transitions(records: tuple[FrameRecord, ...]) -> list[SessionState]:
    """Domain state after every frame — the sequence R31 is a claim about.

    Deliberately the same three calls ``TalariaApp.ingest`` makes, in the same
    order, rather than a simplified stand-in: a comparison harness that folds
    differently from the app compares something the app never does.
    """
    state = SessionState()
    out: list[SessionState] = []
    for record in records:
        if record.direction == "out":
            continue
        decoded = normalize_frame(
            record.frame, at=record.at, seq=record.seq, parse_error=record.parse_error
        )
        state = apply_frame(state, decoded)
        out.append(state)
    return out


async def replay_records(path: Path) -> tuple[FrameRecord, ...]:
    source = ReplaySource.from_path(path, controls=ReplayControls(speed=float("inf")))
    collected = [record async for record in source]
    await source.close()
    return tuple(collected)


async def live_records(
    gateway: StubGateway, *, expected: int, clock: Any
) -> tuple[FrameRecord, ...]:
    source = LiveSource(
        AttachTarget.from_url(gateway.url), FixedCredential(), clock=clock
    )
    collected: list[FrameRecord] = []

    async def _collect() -> None:
        async for record in source:
            collected.append(record)
            if len(collected) >= expected:
                return

    try:
        await asyncio.wait_for(_collect(), timeout=15)
    finally:
        await source.close()
    return tuple(collected)


# ── the equivalence relation ─────────────────────────────────────────────


@pytest.mark.asyncio
async def test_identical_ordered_frames_produce_identical_transitions(
    tmp_path: Path,
) -> None:
    """AE16, stated strictly: every intermediate state, not just the last one.

    **The transition comparison is against an independently built expectation,
    not against the other source.** It used to assert ``from_socket == from_file``
    and then compare ``transitions()`` of those same two values — a pure function
    of an equality that had just been asserted, so the second comparison could not
    fail on its own and the file's headline claim rested on one line. The expected
    sequence is now folded from the corpus definition itself, so a live source
    that dropped, reordered, or renumbered a frame fails the transition
    comparison whatever the replay source did. The record-level equality is kept
    as a separate, stronger statement about the two sources agreeing on the
    fields ``transitions()`` never reads.

    One value is still borrowed from the replay run: ``at``, because the recorded
    timestamps live in the log's ISO strings and re-deriving them here would be a
    second implementation of the replay parser. That is the variable this test
    pins rather than compares (see the module docstring).
    """
    frames = corpus()
    log = write_log(tmp_path / "corpus.jsonl", frames)

    from_file = await replay_records(log)
    assert len(from_file) == len(frames)

    recorded_times = [record.at for record in from_file]
    gateway = StubGateway(greeting=frames)
    await gateway.start()
    try:
        clock = iter(recorded_times)
        from_socket = await live_records(
            gateway, expected=len(frames), clock=lambda: next(clock)
        )
    finally:
        await gateway.stop()

    expected_records = tuple(
        FrameRecord(seq=index + 1, at=at, direction="in", frame=frame)
        for index, (frame, at) in enumerate(zip(frames, recorded_times, strict=True))
    )
    expected = transitions(expected_records)
    lived = transitions(from_socket)
    assert len(lived) == len(expected)
    for index, (left, right) in enumerate(zip(lived, expected, strict=True)):
        assert left == right, f"the live source diverged from the corpus at frame {index + 1}"

    assert from_socket == from_file, "the seam handed the two sources different records"
    assert project(lived[-1], mode="replay") == project(
        transitions(from_file)[-1], mode="replay"
    )


@pytest.mark.asyncio
async def test_the_projection_agrees_even_on_a_real_wall_clock(
    tmp_path: Path,
) -> None:
    """The pinned clock is isolating a known difference, not hiding a defect.

    On a real clock the two sources genuinely disagree about observation times —
    that is the design — so this asserts the parts that carry no time: the
    transcript, the prompts, the sub-agent identities and statuses, and the
    whole frozen status payload.
    """
    frames = corpus()
    log = write_log(tmp_path / "corpus.jsonl", frames)

    from_file = await replay_records(log)
    gateway = StubGateway(greeting=frames)
    await gateway.start()
    try:
        import time

        from_socket = await live_records(gateway, expected=len(frames), clock=time.time)
    finally:
        await gateway.stop()

    replayed = transitions(from_file)[-1]
    lived = transitions(from_socket)[-1]

    assert lived != replayed, "the wall-clock run should differ somewhere, or this proves nothing"
    assert transcript_view(lived) == transcript_view(replayed)
    assert lived.prompts == replayed.prompts
    assert [(row.id, row.status, row.detail) for row in lived.subagents] == [
        (row.id, row.status, row.detail) for row in replayed.subagents
    ]
    assert lived.turn == replayed.turn
    assert lived.usage == replayed.usage
    assert lived.unknown_event_types == replayed.unknown_event_types
    assert lived.cross_session_events_ignored == replayed.cross_session_events_ignored
    assert lived.late_events_ignored == replayed.late_events_ignored

    left = project(lived, mode="live").status
    right = project(replayed, mode="live").status
    assert left == right


@pytest.mark.asyncio
async def test_the_assembled_interface_renders_the_same_transcript_from_both(
    tmp_path: Path,
) -> None:
    """The claim carried up through the real app, not only the reducer."""
    frames = corpus()
    log = write_log(tmp_path / "corpus.jsonl", frames)

    replay_app = TalariaApp(
        ReplaySource.from_path(log, controls=ReplayControls(speed=float("inf"))),
        mode="replay",
    )
    async with replay_app.run_test():
        await replay_app.drain(timeout=15)
        replay_lines = list(replay_app.transcript.rendered_lines)
        await replay_app.shutdown_sources()

    recorded_times = iter([record.at for record in await replay_records(log)])
    gateway = StubGateway(greeting=frames)
    await gateway.start()
    try:
        source = LiveSource(
            AttachTarget.from_url(gateway.url),
            FixedCredential(),
            clock=lambda: next(recorded_times),
        )
        live = TalariaApp(source, mode="live", dispatcher=source)
        async with live.run_test():

            async def _folded() -> None:
                while live.frames_applied < len(frames):
                    await asyncio.sleep(0.005)

            await asyncio.wait_for(_folded(), timeout=15)
            live._dirty = True
            await live.render_snapshot()
            live_lines = list(live.transcript.rendered_lines)
            await live.shutdown_sources()
    finally:
        await gateway.stop()

    assert live_lines == replay_lines


# ── AE16's measured half: the live path's own numbers ────────────────────


#: How long the streaming measurement below runs for. A rate is only a rate over
#: a window: KTD14 specifies a 60-second window, and firing 2,000 frames as fast
#: as loopback allows produced "32 flushes per second" from **two** flushes in
#: 61 milliseconds — a number with no information in it. The window here is
#: shorter than KTD14's (the suite has to stay runnable) but long enough that the
#: 50ms coalescing boundary has fired dozens of times, which is what makes the
#: ceiling a real bound rather than an artifact of division.
STREAM_WINDOW_SECONDS = 1.5
STREAM_BATCHES = 20


@pytest.mark.asyncio
async def test_the_live_path_streams_without_loss_and_within_the_render_ceiling() -> None:
    """Measured on the live path rather than inherited from replay (AE16).

    Two of KTD14's thresholds are meaningful at this corpus size — zero content
    loss and the 25-flushes-per-second render ceiling. The mounted-widget and
    resident-memory bounds are not: they are properties of a 50,000-delta corpus
    and are measured by U5's gate, which this test does not duplicate.

    The frames are *paced* rather than dumped, which is also what a gateway
    does: Hermes coalesces per-token frames on a ~33ms timer of its own
    (``tui_gateway/ws.py``). Dumping the whole corpus into the socket at once
    would measure how fast loopback is.
    """
    deltas = 2_000
    batch = deltas // STREAM_BATCHES
    gateway = StubGateway(greeting=[READY_FRAME, event("message.start", {})])
    await gateway.start()
    try:
        source = LiveSource(AttachTarget.from_url(gateway.url), FixedCredential())
        app = TalariaApp(source, mode="live", dispatcher=source)
        async with app.run_test():
            await gateway.wait_for_attach()

            index = 0
            for _ in range(STREAM_BATCHES):
                await gateway.send_all(
                    [
                        event("message.delta", {"text": f"tok{index + step} "})
                        for step in range(batch)
                    ]
                )
                index += batch
                await asyncio.sleep(STREAM_WINDOW_SECONDS / STREAM_BATCHES)
            await gateway.send(event("message.complete", {"text": ""}))

            expected = deltas + 3

            async def _folded() -> None:
                while app.frames_applied < expected:
                    await asyncio.sleep(0.005)

            await asyncio.wait_for(_folded(), timeout=60)
            app._dirty = True
            await app.render_snapshot()
            measured = app.measurements()
            snapshot: Snapshot | None = app.snapshot
            assert snapshot is not None
            complete = content_is_complete(app.state, snapshot.transcript)
            await app.shutdown_sources()
    finally:
        await gateway.stop()

    assert measured["frames_applied"] == expected
    assert complete, "the live path lost transcript content"
    assert measured["elapsed_seconds"] >= STREAM_WINDOW_SECONDS, measured
    assert measured["render_ticks_per_second"] <= 25.0, measured
    # Coalescing, stated as the property rather than as a rate: two thousand
    # deltas must not have produced two thousand renders.
    assert measured["render_ticks"] < deltas // 10, measured
    assert source.frames_received == expected
    assert source.peak_queued_frames <= 1_000


# ── R30's neighbour: the replay path loads no socket library ─────────────


def test_importing_the_replay_source_does_not_import_websockets() -> None:
    """A fresh interpreter, because pytest has already imported everything.

    R30 says the whole interface runs from a file with no socket open. This is
    the structural half of that: U7 put ``LiveSource`` in the module the replay
    path imports, so the socket library is loaded lazily — and a lazy import is
    exactly the kind of thing a later edit undoes without anyone noticing.
    """
    probe = (
        "import sys; import talaria.replay.source; "
        "assert 'websockets' not in sys.modules, sorted(m for m in sys.modules "
        "if m.startswith('websockets')); print('clean')"
    )
    result = subprocess.run(  # noqa: S603 - fixed argv, no shell
        [sys.executable, "-c", probe],
        capture_output=True,
        text=True,
        cwd=str(Path(__file__).resolve().parents[2]),
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert "clean" in result.stdout
