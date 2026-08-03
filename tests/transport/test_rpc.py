"""KTD13's correlation rules, tested where the races are reachable.

Everything here runs against :class:`RpcCorrelator` directly, with no socket.
That is the point of the module having no I/O: the two failures worth catching —
an interrupted call reported as a success, and a stale reply satisfying a reused
identifier — are both timing bugs, and a test that has to arrange them through a
real socket is a test that arranges them *sometimes*.

``tests/transport/test_reconnect.py`` then proves the same two rules end to end
over the stub gateway, so neither is only true in isolation.
"""

from __future__ import annotations

import asyncio

import pytest

from talaria.transport.rpc import (
    LOST_WITH_TRANSPORT,
    NO_REPLY_IN_TIME,
    RpcCorrelator,
    RpcOutcome,
    RpcRequest,
    unknown_outcome,
)


def reply(request: RpcRequest, result: dict[str, object]) -> dict[str, object]:
    return {"jsonrpc": "2.0", "id": request.id, "result": result}


def refusal(request: RpcRequest, code: int, message: str) -> dict[str, object]:
    return {"jsonrpc": "2.0", "id": request.id, "error": {"code": code, "message": message}}


# ── the ordinary path ────────────────────────────────────────────────────


def test_the_request_frame_matches_the_gateway_shape() -> None:
    request = RpcRequest(epoch=1, id="7", method="prompt.submit", params={"text": "hi"})
    assert request.to_frame() == {
        "jsonrpc": "2.0",
        "id": "7",
        "method": "prompt.submit",
        "params": {"text": "hi"},
    }


@pytest.mark.asyncio
async def test_a_reply_resolves_the_call_it_belongs_to() -> None:
    correlator = RpcCorrelator()
    correlator.open_epoch()

    first, first_future = correlator.begin("session.info")
    second, second_future = correlator.begin("prompt.submit", {"text": "hi"})
    assert first.id != second.id

    assert correlator.resolve(reply(second, {"queued": True}), epoch=1) is True
    outcome = await second_future
    assert outcome.confirmed and outcome.result == {"queued": True}
    assert not first_future.done(), "an unrelated call was resolved"

    correlator.resolve(reply(first, {"title": "t"}), epoch=1)
    assert (await first_future).method == "session.info"
    assert correlator.in_flight == 0


@pytest.mark.asyncio
async def test_an_error_reply_is_an_error_not_a_success() -> None:
    correlator = RpcCorrelator()
    correlator.open_epoch()
    request, future = correlator.begin("session.interrupt")

    correlator.resolve(refusal(request, 4009, "session busy"), epoch=1)
    outcome = await future

    assert outcome.status == "error"
    assert outcome.confirmed is False
    assert outcome.error_code == 4009
    assert "session busy" in outcome.notice


@pytest.mark.asyncio
async def test_a_malformed_error_member_is_still_not_a_success() -> None:
    """Reading past a broken ``error`` into ``result`` would invent a success."""
    correlator = RpcCorrelator()
    correlator.open_epoch()
    request, future = correlator.begin("prompt.submit")

    correlator.resolve(
        {"jsonrpc": "2.0", "id": request.id, "error": "boom", "result": {"ok": True}},
        epoch=1,
    )
    outcome = await future
    assert outcome.status == "error"
    assert outcome.confirmed is False


@pytest.mark.asyncio
async def test_a_frame_that_is_not_a_reply_matches_nothing() -> None:
    correlator = RpcCorrelator()
    correlator.open_epoch()
    correlator.begin("session.info")

    assert correlator.resolve({"method": "event", "params": {}}, epoch=1) is False
    assert correlator.resolve("not a frame at all", epoch=1) is False
    # A parse-error reply carries ``id: null`` and answers no specific request
    # (``tui_gateway/ws.py``); treating it as an answer would resolve whichever
    # call happened to be outstanding.
    assert correlator.resolve({"error": {"code": -32700}, "id": None}, epoch=1) is False
    assert correlator.in_flight == 1
    assert correlator.unmatched_replies == 0


def test_a_reply_for_an_unknown_id_is_counted_not_matched() -> None:
    correlator = RpcCorrelator()
    correlator.open_epoch()
    assert correlator.resolve({"jsonrpc": "2.0", "id": "99", "result": {}}, epoch=1) is False
    assert correlator.unmatched_replies == 1


@pytest.mark.asyncio
async def test_an_event_carrying_an_id_and_a_result_is_not_a_reply() -> None:
    """The envelope decides, not the presence of ``id`` and ``result``.

    Accepting any object with a top-level ``id`` and ``result`` was a real
    defect, not a theoretical one: an inbound
    ``{"type": "tool.complete", "id": "1", "result": {…}}`` resolved an in-flight
    ``session.interrupt`` as a confirmed success, and a confirmed interrupt drives
    the turn to ``cancelled``. ``cancelled`` is sticky and suppresses every later
    delta, so the rest of a turn that never stopped would vanish from the screen —
    the exact harm ``interrupt_live``'s docstring says must never happen.
    """
    correlator = RpcCorrelator()
    correlator.open_epoch()
    request, future = correlator.begin("session.interrupt")

    impostors: list[dict[str, object]] = [
        # An event that happens to carry both members.
        {"type": "tool.complete", "id": request.id, "result": {"status": "interrupted"}},
        # A request from the gateway. Replies never carry ``method``.
        {"jsonrpc": "2.0", "method": "ping", "id": request.id, "result": {}},
        # No envelope at all.
        {"id": request.id, "result": {"status": "interrupted"}},
        # An envelope from a protocol this client does not speak.
        {"jsonrpc": "1.0", "id": request.id, "result": {"status": "interrupted"}},
    ]
    for frame in impostors:
        assert correlator.resolve(frame, epoch=1) is False, frame
        assert not future.done(), f"a non-reply resolved a real call: {frame}"

    assert correlator.in_flight == 1
    assert correlator.unmatched_replies == 0

    # The genuine article still resolves, so this is a filter and not a wall.
    assert correlator.resolve(reply(request, {"status": "interrupted"}), epoch=1) is True
    assert (await future).confirmed


def test_beginning_a_call_before_any_dial_is_refused() -> None:
    """An id minted against epoch 0 could never be answered by anything."""
    correlator = RpcCorrelator()
    with pytest.raises(RuntimeError, match="dial before"):
        correlator.begin("prompt.submit")


# ── AE8: an interrupted call resolves to unknown ─────────────────────────


@pytest.mark.asyncio
async def test_a_disconnect_resolves_every_in_flight_call_to_unknown() -> None:
    correlator = RpcCorrelator()
    correlator.open_epoch()
    _, submit = correlator.begin("prompt.submit")
    _, interrupt = correlator.begin("session.interrupt")

    assert correlator.abandon(1, LOST_WITH_TRANSPORT) == 2

    for future in (submit, interrupt):
        outcome = await future
        assert outcome.status == "unknown"
        assert outcome.confirmed is False
        assert outcome.unknown is True
        assert LOST_WITH_TRANSPORT in outcome.notice
        assert "may or may not" in outcome.notice
    assert correlator.lost_outcomes == 2
    assert correlator.in_flight == 0


@pytest.mark.asyncio
async def test_a_discarded_call_is_unknown_and_counted() -> None:
    correlator = RpcCorrelator()
    correlator.open_epoch()
    request, _ = correlator.begin("prompt.submit")

    outcome = correlator.discard(request, NO_REPLY_IN_TIME)
    assert outcome.status == "unknown"
    assert correlator.lost_outcomes == 1
    assert correlator.is_pending(1, request.id) is False


def test_an_outcome_never_claims_more_than_it_knows() -> None:
    """The word 'sent' must not appear on anything but a confirmed reply."""
    lost = unknown_outcome("prompt.submit", LOST_WITH_TRANSPORT, epoch=1)
    assert lost.confirmed is False
    assert "unknown" in lost.notice

    confirmed = RpcOutcome(status="ok", method="prompt.submit", request_id="1", epoch=1)
    assert confirmed.confirmed is True
    assert confirmed.notice == ""


# ── KTD13's epoch rule: the reconnect race ───────────────────────────────


@pytest.mark.asyncio
async def test_a_stale_epoch_reply_cannot_resolve_a_reused_identifier() -> None:
    """The failure this whole mechanism exists for.

    Sequence: a call goes out on epoch 1 and the socket drops; the call is
    honestly abandoned as ``unknown``; a reconnect opens epoch 2 and — because
    ids restart per connection — the next call is minted with the *same* id;
    the dead socket's reply for epoch 1 then arrives. Keyed on id alone it
    resolves the new call into a success it never had.
    """
    correlator = RpcCorrelator()
    correlator.open_epoch()
    first, first_future = correlator.begin("prompt.submit", {"text": "the first message"})

    correlator.abandon(1, LOST_WITH_TRANSPORT)
    assert (await first_future).status == "unknown"

    correlator.open_epoch()
    second, second_future = correlator.begin("prompt.submit", {"text": "the second message"})
    assert second.id == first.id, "ids must be reusable, or this race is unreachable"
    assert second.epoch == 2

    # The dead connection's reply, arriving late.
    matched = correlator.resolve(reply(first, {"queued": True}), epoch=1)

    assert matched is False
    assert correlator.stale_epoch_replies == 1
    assert not second_future.done(), "a stale reply resolved the reused identifier"

    correlator.resolve(reply(second, {"queued": True}), epoch=2)
    outcome = await second_future
    assert outcome.confirmed and outcome.epoch == 2


@pytest.mark.asyncio
async def test_a_stale_reply_for_a_call_still_pending_on_the_dead_epoch_is_discarded() -> None:
    """Even without an abandon first, the epoch alone decides."""
    correlator = RpcCorrelator()
    correlator.open_epoch()
    first, first_future = correlator.begin("session.info")
    correlator.open_epoch()

    assert correlator.resolve(reply(first, {"title": "t"}), epoch=1) is False
    assert correlator.stale_epoch_replies == 1
    assert not first_future.done()

    # And it is still abandonable afterwards, as an unknown.
    correlator.abandon(1, LOST_WITH_TRANSPORT)
    assert (await first_future).status == "unknown"


@pytest.mark.asyncio
async def test_abandon_all_covers_every_open_epoch() -> None:
    correlator = RpcCorrelator()
    correlator.open_epoch()
    _, first = correlator.begin("a.one")
    correlator.open_epoch()
    _, second = correlator.begin("b.two")

    assert correlator.abandon_all("the transport was closed") == 2
    assert (await first).status == "unknown"
    assert (await second).status == "unknown"


@pytest.mark.asyncio
async def test_identifiers_restart_on_every_epoch() -> None:
    """Documented behaviour, pinned: the epoch guard needs reuse to guard against."""
    correlator = RpcCorrelator()
    correlator.open_epoch()
    first, _ = correlator.begin("a.one")
    correlator.abandon_all("closed")

    correlator.open_epoch()
    second, _ = correlator.begin("a.one")
    correlator.abandon_all("closed")

    assert (first.epoch, first.id) == (1, "1")
    assert (second.epoch, second.id) == (2, "1")
    await asyncio.sleep(0)
