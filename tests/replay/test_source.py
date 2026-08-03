"""The frame-log-backed source, and R30's "no socket" claim.

R30 is the requirement the whole replay-first ordering rests on, and it is easy
to assert weakly. "No gateway was running" proves nothing — the process could
still have tried. So the test below arms ``socket.socket.connect`` to raise and
then runs a complete replay through the assembled app: if any layer dialled
anything, the run fails with that exception rather than passing quietly.
"""

from __future__ import annotations

import json
import socket
from pathlib import Path
from typing import Any

import pytest

from talaria.recorder.reader import FrameLogError
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource, load_frame_records, load_header
from talaria.ui.app import TalariaApp
from tests.ui.conftest import event, streaming_turn


def _write_log(path: Path, frames: list[Any], *, with_hole: bool = False) -> Path:
    lines = [
        json.dumps(
            {
                "kind": "header",
                "version": 1,
                "startedAt": "2026-08-02T12:21:35.016Z",
                "endpoint": "ws://127.0.0.1:9119/api/ws?token=%5Bredacted%5D",
            }
        )
    ]
    for index, frame in enumerate(frames):
        record: dict[str, Any] = {
            "kind": "frame",
            "seq": index + 1,
            "at": f"2026-08-02T12:21:{35 + index // 100:02d}.{(index * 4) % 1000:03d}Z",
            "dir": "in",
            "frame": frame,
        }
        lines.append(json.dumps(record))
    if with_hole:
        lines.append(
            json.dumps(
                {
                    "kind": "frame",
                    "seq": len(frames) + 1,
                    "at": "2026-08-02T12:21:40.000Z",
                    "dir": "in",
                    "frame": None,
                    "parseError": "payload was not valid JSON",
                }
            )
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def test_a_frame_log_loads_into_seam_records(tmp_path: Path) -> None:
    frames = [event("gateway.ready", {}), *streaming_turn(["a ", "b "])]
    path = _write_log(tmp_path / "corpus.jsonl", frames, with_hole=True)

    header = load_header(path)
    assert header.version == 1
    assert "token=%5Bredacted%5D" in header.endpoint

    loaded = load_frame_records(path)
    assert len(loaded) == len(frames) + 1
    assert loaded[0].seq == 1
    assert loaded[0].at > 0
    assert loaded[0].direction == "in"
    # The withheld hole crosses the seam as a flag, never as the recorder's text.
    assert loaded[-1].parse_error is True
    assert loaded[-1].frame is None


def test_a_file_without_a_header_is_rejected(tmp_path: Path) -> None:
    path = tmp_path / "bad.jsonl"
    path.write_text('{"kind": "frame", "seq": 1}\n', encoding="utf-8")
    with pytest.raises(FrameLogError):
        load_frame_records(path)


@pytest.mark.asyncio
async def test_the_whole_interface_runs_from_a_file_with_no_socket_opened(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """R30, asserted by arming the dialler rather than by observing its absence."""
    frames = [event("gateway.ready", {})]
    for turn in range(5):
        frames.extend(streaming_turn([f"turn {turn} chunk {n} " for n in range(12)]))
    path = _write_log(tmp_path / "corpus.jsonl", frames)

    def refuse(*args: object, **kwargs: object) -> None:
        raise AssertionError("replay opened a socket")

    monkeypatch.setattr(socket.socket, "connect", refuse)
    monkeypatch.setattr(socket.socket, "connect_ex", refuse)

    controls = ReplayControls()
    controls.set_unbounded()
    source = ReplaySource.from_path(path, controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test(size=(100, 30)) as pilot:
        await app.drain(timeout=60.0)
        await pilot.pause()
        assert app.frames_applied == len(frames)
        assert "turn 4 chunk 11" in app.snapshot.transcript.text  # type: ignore[union-attr]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_closing_the_source_stops_a_paused_iteration(tmp_path: Path) -> None:
    """A paused source parked on its resume event must not hang teardown (R36)."""
    path = _write_log(tmp_path / "corpus.jsonl", [event("gateway.ready", {})] * 20)
    controls = ReplayControls(paused=True)
    source = ReplaySource.from_path(path, controls=controls)
    iterator = source.__aiter__()
    await source.close()
    with pytest.raises(StopAsyncIteration):
        await iterator.__anext__()
    assert source.closed is True
