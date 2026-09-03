"""Shared fixtures for the Pilot-driven UI suites.

Every test here drives the real :class:`~talaria.ui.app.TalariaApp` over a real
:class:`~talaria.replay.source.ReplaySource`. There is no widget-in-isolation
harness on purpose: the gate's claim is about the assembled interface, and a
test that mounts one widget by hand proves something the gate never asserted.
"""

from __future__ import annotations

import html
import re
from collections.abc import Iterator, Mapping
from typing import Any

import pytest

from talaria.domain.commands import CATALOG_METHOD
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.rpc import RpcOutcome
from talaria.transport.source import FrameRecord
from talaria.ui.app import LiveDispatcher, TalariaApp

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


class RecordingDispatcher:
    """A dispatcher double that records calls and returns a chosen outcome."""

    def __init__(self, outcome: RpcOutcome | None = None) -> None:
        self.outcome = outcome
        self.calls: list[tuple[str, Mapping[str, Any]]] = []

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if self.outcome is not None:
            return self.outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    @property
    def operator_calls(self) -> list[tuple[str, Mapping[str, Any]]]:
        """Every call except the startup catalogue fetch.

        ``TalariaApp`` reads ``commands.catalog`` once when it mounts in live
        mode (U9), so a raw ``calls`` list starts with a call no operator made.
        These tests are about what one operator action sent, so they read this;
        ``calls`` stays raw, and the fetch itself is asserted over a real socket
        in ``tests/transport/test_commands.py``.
        """
        return [call for call in self.calls if call[0] != CATALOG_METHOD]


def live_app(
    dispatcher: LiveDispatcher,
    *,
    coalesce_interval: float = 3600.0,
    connections: Any = None,
    **kwargs: Any,
) -> TalariaApp:
    """A live-mode app whose only renders are the ones a test asks for.

    Typed against :class:`~talaria.ui.app.LiveDispatcher` — the structural
    protocol :class:`RecordingDispatcher` already satisfies — rather than
    against that one concrete double, so a test module that needs a dispatcher
    answering more than one fixed outcome (``tests/ui/test_sessions.py``'s
    ``ScriptedDispatcher``) can build its own double and still call this
    helper.

    ``coalesce_interval`` is parked beyond any test's lifetime by default, on
    purpose. Every assertion here is about what is on screen *after* a specific
    state change, and leaving the 50ms tick armed makes each of those a bet on
    whether the timer fired inside ``pilot.pause()`` — a passing test today and
    a flake under whole-suite load, which is the shape DECISIONS.md already
    records three instances of. :func:`settle` renders explicitly instead.

    ``connections`` is the fleet seam, defaulting to ``None`` — the
    single-connection shape every caller of this helper had before U7. A test
    that passes one gets the per-connection paths, each pinned where it is
    asserted rather than claimed here: ``/profiles`` ensures beside rather than
    drop-switching (``tests/ui/test_picker.py``'s
    ``test_selecting_a_profile_ensures_its_connection_and_never_switches``),
    background connections are probed and swept
    (``test_a_background_connection_is_probed_and_swept_together``), and each
    channel takes its generation from its own socket
    (``test_a_background_rows_liveness_survives_a_busier_focused_connection``).

    **The coalesce parameter exists because parking the timer also parks a whole class
    of defect.** Anything that re-arms itself from inside the render pass is
    invisible to a suite that only ever renders on demand: a terminal-read that
    put itself back in the registry ran once per test here and 136 times in
    400ms with the timer live. A test about a loop has to let the loop turn.
    """
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    return TalariaApp(
        source,
        mode="live",
        controls=controls,
        dispatcher=dispatcher,
        coalesce_interval=coalesce_interval,
        connections=connections,
        **kwargs,
    )


def feed(app: TalariaApp, frame: dict[str, Any], *, seq: int = 100) -> None:
    """Fold one frame the way the pump would, without running the pump."""
    app.ingest(
        FrameRecord(seq=seq, at=1_785_000_000.0 + seq, direction="in", frame=frame)
    )


async def settle(app: TalariaApp, pilot: Any) -> None:
    """Render once, drain any live call it started, and render the result."""
    await app.render_snapshot()
    await pilot.pause()
    await app.settle_live()
    await app.render_snapshot()
    await pilot.pause()


@pytest.fixture
def stress_frames() -> Iterator[list[dict[str, Any]]]:
    """Enough completed turns to push a small mount cap well past its limit."""
    frames: list[dict[str, Any]] = [event("gateway.ready", {})]
    for turn in range(40):
        frames.extend(streaming_turn([f"line {turn}.{step}\n" for step in range(6)]))
    yield frames
