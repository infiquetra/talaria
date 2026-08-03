"""Shared fixtures for the Pilot-driven UI suites.

Every test here drives the real :class:`~talaria.ui.app.TalariaApp` over a real
:class:`~talaria.replay.source.ReplaySource`. There is no widget-in-isolation
harness on purpose: the gate's claim is about the assembled interface, and a
test that mounts one widget by hand proves something the gate never asserted.
"""

from __future__ import annotations

from collections.abc import Iterator
from typing import Any

import pytest

from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.source import FrameRecord
from talaria.ui.app import TalariaApp

#: Recorded seconds between synthetic frames, matching the stress generator.
STEP = 0.004


def event(kind: str, payload: dict[str, Any], *, session: str = "s1") -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": kind, "session_id": session, "payload": payload},
    }


def records(frames: list[Any], *, start: float = 1_785_000_000.0) -> tuple[FrameRecord, ...]:
    """Wrap raw frames as seam records with a monotonic recorded clock."""
    return tuple(
        FrameRecord(seq=index + 1, at=start + index * STEP, direction="in", frame=frame)
        for index, frame in enumerate(frames)
    )


def streaming_turn(text_parts: list[str], *, session: str = "s1") -> list[dict[str, Any]]:
    """A complete turn: start, deltas, complete."""
    frames = [event("message.start", {}, session=session)]
    frames.extend(event("message.delta", {"text": part}, session=session) for part in text_parts)
    frames.append(
        event("message.complete", {"text": "".join(text_parts)}, session=session)
    )
    return frames


def paused_app(frames: list[Any], **kwargs: Any) -> tuple[TalariaApp, ReplayControls]:
    """An app whose source is parked, so a test drives frames explicitly."""
    controls = ReplayControls(paused=True)
    source = ReplaySource(records(frames), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls, **kwargs)
    return app, controls


@pytest.fixture
def stress_frames() -> Iterator[list[dict[str, Any]]]:
    """Enough completed turns to push a small mount cap well past its limit."""
    frames: list[dict[str, Any]] = [event("gateway.ready", {})]
    for turn in range(40):
        frames.extend(streaming_turn([f"line {turn}.{step}\n" for step in range(6)]))
    yield frames
