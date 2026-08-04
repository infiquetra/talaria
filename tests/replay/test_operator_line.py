"""A replay rebuilds the question, not just the answer (R3, R30).

R3's evidence method is stated in ``talaria/cli.py``: one live turn streamed to
completion, and its transcript compared against a replay of the same frames.
That comparison could not complete. The gateway never echoes a submitted prompt
back as an event, so the operator's own line is written locally at submit time —
and a replay never submits anything. Replaying a recording of a real session
rebuilt the agent's half of the conversation and left out the question it was
answering, which under R30 ("drives the entire interface from a frame log") is
an interface driven to a visibly incomplete state.

The text was never missing from the recording. It is in the *outbound* half of
the frame log, in the ``prompt.submit`` the composer sent, which the replay path
discarded along with every other request.

Two claims are tested here and they pull against each other, which is the point:
a replay must write that line, and a live session must not write it twice.
"""

from __future__ import annotations

from typing import Any

import pytest

from talaria.domain.state import (
    SUBMIT_METHOD,
    SessionState,
    record_replayed_submission,
    replayed_submission_text,
)
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.rpc import RpcOutcome
from talaria.ui.app import TalariaApp
from tests.ui.conftest import RecordingDispatcher, event, records

PROMPT = "Reply with exactly one word: ALPHA"


def submit_frame(text: str = PROMPT, *, session: str = "s1") -> dict[str, Any]:
    """The frame the composer's dispatcher writes, as a recorder stores it."""
    return {
        "jsonrpc": "2.0",
        "id": "8",
        "method": SUBMIT_METHOD,
        "params": {"session_id": session, "text": text},
    }


def outbound(frames: list[Any], *, at: int) -> Any:
    """Re-stamp one record as outbound, which is how a recording holds a request."""
    from dataclasses import replace as dc_replace

    return dc_replace(frames[at], direction="out")


# ── reading the operator's words back out of a request ───────────────────


def test_the_submitted_text_is_recovered_from_the_recorded_request() -> None:
    assert replayed_submission_text(submit_frame()) == PROMPT


@pytest.mark.parametrize(
    "frame",
    [
        {"method": "session.interrupt", "params": {"session_id": "s1"}},
        {"method": SUBMIT_METHOD},
        {"method": SUBMIT_METHOD, "params": None},
        {"method": SUBMIT_METHOD, "params": {"session_id": "s1"}},
        {"method": SUBMIT_METHOD, "params": {"text": ""}},
        {"method": SUBMIT_METHOD, "params": {"text": 7}},
        {"params": {"text": "no method"}},
        "not a frame at all",
        None,
    ],
)
def test_anything_that_is_not_a_submitted_prompt_reads_as_nothing(frame: Any) -> None:
    """Every outbound frame is offered to this, so it has to answer for all of
    them — including the malformed ones a recorder will happily store."""
    assert replayed_submission_text(frame) is None


def test_a_replayed_line_claims_nothing_about_delivery() -> None:
    """``record_submission`` can write a delivery note because a live caller
    observed an outcome. A replay observed nothing — the acknowledgement, if one
    came, is a later frame — so this writes the operator's words and stops."""
    state = record_replayed_submission(SessionState(), PROMPT, at=5.0)
    assert [(e.kind, e.text) for e in state.transcript] == [("user", PROMPT)]
    assert state.last_observed_at == 5.0


# ── through the assembled app, in both modes ─────────────────────────────


def _session(text: str) -> list[Any]:
    return [
        event("gateway.ready", {}),
        submit_frame(text),
        event("message.start", {}),
        event("message.delta", {"text": "ALPHA"}),
        event("message.complete", {"text": "ALPHA"}),
    ]


def _replay_app(text: str = PROMPT) -> TalariaApp:
    frames = list(records(_session(text)))
    frames[1] = outbound(frames, at=1)
    controls = ReplayControls(speed=0.0)
    return TalariaApp(ReplaySource(tuple(frames), controls=controls), mode="replay",
                      controls=controls)


@pytest.mark.asyncio
async def test_a_replayed_session_shows_the_prompt_and_the_reply() -> None:
    """The whole point: both halves of the conversation, in order."""
    app = _replay_app()
    async with app.run_test(size=(100, 24)) as pilot:
        await app.drain(timeout=30.0)
        await pilot.pause()
        assert [(e.kind, e.text) for e in app.state.transcript] == [
            ("user", PROMPT),
            ("assistant", "ALPHA"),
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_live_session_writes_the_operator_line_exactly_once() -> None:
    """The constraint that makes this mode-scoped rather than unconditional.

    In live mode ``submit_live`` has already written the line before the frame
    is recorded, so folding the request as well would print the operator's
    message twice — which is a worse defect than the one being fixed, because a
    duplicated line reads as a message that was actually sent twice.
    """
    dispatcher = RecordingDispatcher(
        RpcOutcome(status="ok", method=SUBMIT_METHOD, request_id="1", epoch=1, result={})
    )
    controls = ReplayControls(paused=True)
    app = TalariaApp(
        ReplaySource(records([event("gateway.ready", {})]), controls=controls),
        mode="live",
        dispatcher=dispatcher,
    )
    async with app.run_test(size=(100, 24)) as pilot:
        await pilot.pause()
        await app.submit_live(PROMPT)
        await pilot.pause()

        # And now the recorder's copy of that same request arrives at ``ingest``,
        # which is exactly what a live source with a recorder attached does.
        app.ingest(outbound(list(records([submit_frame()])), at=0))
        await pilot.pause()

        assert [e.text for e in app.state.transcript if e.kind == "user"] == [PROMPT]
        await app.shutdown_sources()
