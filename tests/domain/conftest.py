"""Frame builders for the domain suite.

These construct the *raw* shape a gateway frame has on the wire —
``{"method": "event", "params": {"type": ..., "payload": ...}}`` — rather than a
decoded object, so every test exercises :mod:`talaria.domain.decode` on the way
in. A helper that skipped the decoder would let a decoding regression pass every
state test in this directory.

``at`` and ``seq`` are derived from the frame's position, never from a clock:
AE2 requires that replaying one corpus twice produces identical state, and a
builder that stamped ``time.time()`` would make every determinism assertion in
this suite pass for the wrong reason.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from typing import Any

import pytest

from talaria.domain.decode import DecodedFrame, decode_frame
from talaria.domain.state import SessionState, apply_frames

#: Every builder below starts here, so a corpus's times are small, exact, and
#: reproducible across machines and Python versions.
BASE_TIME = 1_754_000_000.0

DEFAULT_SESSION = "sess-focus"


def raw_event(
    event_type: str,
    payload: Mapping[str, Any] | None = None,
    *,
    session_id: str | None = DEFAULT_SESSION,
) -> dict[str, Any]:
    """One raw event frame, exactly as it arrives on the socket."""
    params: dict[str, Any] = {"type": event_type, "payload": dict(payload or {})}
    if session_id is not None:
        params["session_id"] = session_id
    return {"method": "event", "params": params}


def decode_all(frames: Iterable[Any]) -> list[DecodedFrame]:
    """Decode a list of raw frames with deterministic ``at``/``seq`` stamps."""
    return [
        decode_frame(frame, at=BASE_TIME + index, seq=index + 1)
        for index, frame in enumerate(frames)
    ]


def replay(frames: Iterable[Any], state: SessionState | None = None) -> SessionState:
    """Fold a list of raw frames into session state."""
    return apply_frames(state if state is not None else SessionState(), decode_all(frames))


@pytest.fixture
def streaming_turn() -> list[dict[str, Any]]:
    """A minimal complete turn: start, two deltas, and a final message.

    Used by several files. The final text repeats the streamed text, which is
    what the real gateway does and what the final-tail dedupe exists to handle.
    """
    return [
        raw_event("message.start"),
        raw_event("message.delta", {"text": "Hello, "}),
        raw_event("message.delta", {"text": "world."}),
        raw_event("message.complete", {"text": "Hello, world."}),
    ]
