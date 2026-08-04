"""Shared fixtures for the Pilot-driven UI suites.

Every test here drives the real :class:`~talaria.ui.app.TalariaApp` over a real
:class:`~talaria.replay.source.ReplaySource`. There is no widget-in-isolation
harness on purpose: the gate's claim is about the assembled interface, and a
test that mounts one widget by hand proves something the gate never asserted.
"""

from __future__ import annotations

import html
import re
from collections.abc import Iterator
from typing import Any

import pytest

from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.source import FrameRecord
from talaria.ui.app import TalariaApp

#: Recorded seconds between synthetic frames, matching the stress generator.
STEP = 0.004


def screen_text(app: TalariaApp) -> str:
    """Everything the operator can read, as plain text.

    ``App.export_screenshot`` returns an SVG whose glyphs sit inside ``<text>``
    elements with spaces written as ``&#160;``. Every assertion about what is on
    screen goes through here, and the reason to have it rather than to search
    the raw SVG is that ``"once" in svg`` is easy to satisfy by accident while
    ``"once" in screen_text(app)`` is a claim about a rendered row.

    Pair every "this is absent" with a "this is present" from the same string.
    A negative assertion against a screen is satisfied by a blank screen, and a
    blank screen is a real failure mode: a control laid out at zero height is
    mounted, focusable and completely invisible.
    """
    body = re.sub(r"<[^>]+>", "", app.export_screenshot())
    return html.unescape(body).replace("\xa0", " ")


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
