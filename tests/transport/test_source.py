"""KTD7: the typed end-of-stream cause, wired end to end.

Every test here drives a real :class:`~talaria.transport.source.LiveSource`
against the stub gateway, through the real ``LiveSource.bind`` ->
``TalariaApp.note_connection_state`` -> ``set_connection`` path
(``talaria/cli.py:453`` wires the same two calls in production) — a fake
callback would prove the domain transition works and say nothing about
whether the cause the transport computes ever reaches it.

Each of the four typed causes gets one scenario: some content streams, the
source ends for that specific reason, and the domain's transcript shows the
partial content committed rather than dropped (R6). A fifth scenario proves
the opposite: a transient reconnect that resumes the same response commits
nothing extra and the eventual content lands exactly once.
"""

from __future__ import annotations

import asyncio
from typing import Any, get_args

import pytest

from talaria.domain.models import TerminalCause as DomainTerminalCause
from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential
from talaria.transport.source import LiveSource
from talaria.transport.source import TerminalCause as TransportTerminalCause
from talaria.ui.app import TalariaApp
from tests.transport.conftest import STUB_TOKEN, StubGateway, event

FAST_RETRIES = (0.0, 0.01, 0.01)


class ScriptedProvider:
    """A credential provider double that can hand out a different value on
    the next call — the shape ``KTD11``'s per-dial rule requires."""

    def __init__(self, values: list[str]) -> None:
        self.values = values
        self.calls = 0

    async def acquire(self) -> Credential:
        value = self.values[min(self.calls, len(self.values) - 1)]
        self.calls += 1
        return Credential("token", value, "file")


def live_source(gateway: StubGateway, provider: Any = None, **kwargs: Any) -> LiveSource:
    return LiveSource(
        AttachTarget.from_url(gateway.url),
        provider if provider is not None else ScriptedProvider([STUB_TOKEN]),
        reconnect_delays=kwargs.pop("reconnect_delays", FAST_RETRIES),
        **kwargs,
    )


def live_app(source: LiveSource, **kwargs: Any) -> TalariaApp:
    app = TalariaApp(source, mode="live", dispatcher=source, **kwargs)
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)
    return app


async def drain_into(app: TalariaApp, *, until: int, timeout: float = 5.0) -> None:
    async def _poll() -> None:
        while app.frames_applied < until:
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


async def settle(predicate: Any, *, timeout: float = 5.0) -> None:
    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


# ── the two typed-cause enums must not drift ──────────────────────────────


def test_the_transport_and_domain_terminal_cause_enums_are_identical() -> None:
    """The transport re-declares :data:`TerminalCause` rather than importing
    the domain (ADR-0002's boundary) — the same reason
    ``LiveConnectionState``/``ConnectionStatus`` are duplicated, and the same
    cost, paid once, here."""
    assert set(get_args(TransportTerminalCause)) == set(get_args(DomainTerminalCause))


# ── the typed cause, wired end to end, one scenario per cause ────────────


@pytest.mark.asyncio
async def test_auth_failed_mid_stream_commits_the_partial_reply(gateway: StubGateway) -> None:
    """The gateway's post-accept close shape: a credential that stops working
    surfaces through the *disconnect* path (``classify_dial_error``), and
    KTD7's cause travels from there through the live wiring into the
    committed transcript."""
    provider = ScriptedProvider([STUB_TOKEN, "no-longer-valid"])
    source = live_source(gateway, provider)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [event("message.start", {}), event("message.delta", {"text": "partial reply"})]
        )
        await drain_into(app, until=3)
        assert app.state.streaming_text == "partial reply"

        await gateway.hang_up()

        await settle(lambda: app.state.connection == "auth_failed")
        assert [e.text for e in app.state.transcript if e.kind == "assistant"] == [
            "partial reply"
        ], "the typed auth_failed cause did not reach the domain commit"
        assert app.state.streaming_text == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_dial_failed_on_the_first_connect_reaches_the_domain(
    gateway: StubGateway,
) -> None:
    """No content streamed yet, so nothing to commit — this proves the
    wiring does not crash or misfire on the *simplest* typed cause, the one
    every other scenario in this file builds on."""
    await gateway.stop()
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await app.drain(timeout=10)
        assert app.state.connection == "disconnected"
        assert app.state.transcript == ()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_orderly_close_mid_stream_commits_the_partial_reply(gateway: StubGateway) -> None:
    """The operator quitting mid-turn: ``shutdown_sources`` is the exact
    production teardown path (``on_unmount`` calls it), and it must not
    silently lose whatever had already streamed."""
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [
                event("message.start", {}),
                event("reasoning.delta", {"text": "thinking about it"}),
                event("message.delta", {"text": "an unfinished reply"}),
            ]
        )
        await drain_into(app, until=4)
        assert app.state.streaming_text == "an unfinished reply"

        await app.shutdown_sources()

        assert [e.text for e in app.state.transcript if e.kind == "reasoning"] == [
            "thinking about it"
        ]
        assert [e.text for e in app.state.transcript if e.kind == "assistant"] == [
            "an unfinished reply"
        ]
        assert app.state.streaming_text == ""
        assert app.state.reasoning_text == ""


@pytest.mark.asyncio
async def test_reconnect_exhausted_commits_the_partial_reply(gateway: StubGateway) -> None:
    """The gateway never comes back within the reconnect schedule: every
    retry fails, the schedule runs out, and the typed cause this fires must
    still reach the domain commit — not just the earlier, mid-loop failures
    that must *not* commit (those are exercised in ``test_reconnect.py``)."""
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [event("message.start", {}), event("message.delta", {"text": "partial reply"})]
        )
        await drain_into(app, until=3)

        await gateway.stop()
        await gateway.hang_up()

        # ``source.state`` bounces to ``disconnected`` transiently between
        # retries too (``_dial``'s own failure branch, mid-loop) — waiting on
        # the state alone would settle on the wrong moment. What is unique to
        # the schedule actually running out is the committed entry itself, so
        # wait on that directly, with headroom for every retry in the
        # schedule (``FAST_RETRIES`` sums to 20ms, so 5s is generous).
        await settle(
            lambda: any(e.kind == "assistant" for e in app.state.transcript), timeout=5
        )

        assert [e.text for e in app.state.transcript if e.kind == "assistant"] == [
            "partial reply"
        ], "reconnect_exhausted did not commit the partial reply"
        assert source.reconnects == 0
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_transient_reconnect_mid_reply_commits_nothing_extra_and_never_duplicates(
    gateway: StubGateway,
) -> None:
    """The scenario KTD7 draws the line against: a reconnect that *resumes*
    the same response must not have committed the in-flight partial text as
    a spurious entry, and the eventual completed reply must appear exactly
    once — the segment/interim machinery's dedupe backstop, still intact
    end to end."""
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [event("message.start", {}), event("message.delta", {"text": "first half. "})]
        )
        await drain_into(app, until=3)
        # A cause-less transient status change must not have touched the
        # buffer or written any entry. Captured to a local first — asserting
        # ``len(app.state.transcript) == 0`` directly narrows mypy's view of
        # that *expression* to the empty-tuple type for the rest of the
        # block, which then makes every later ``for entry in
        # app.state.transcript`` unable to infer ``entry``'s type at all.
        transcript_before_reconnect = app.state.transcript
        assert app.state.streaming_text == "first half. "
        assert len(transcript_before_reconnect) == 0

        await gateway.hang_up()
        await gateway.wait_for_attach()
        await settle(lambda: app.state.connection == "connected")

        # The reconnect itself commits nothing beyond F6's own pre-existing
        # "reconnected to the gateway" marker (unrelated to KTD7 — a
        # cause-less status change, unaffected by this unit): no reasoning or
        # assistant entry, and the buffer's content survived the round trip
        # untouched.
        transcript_after_reconnect = app.state.transcript
        kinds_after_reconnect: list[str] = [
            entry.kind for entry in transcript_after_reconnect
        ]
        assert kinds_after_reconnect == ["system"], (
            "the reconnect committed something beyond its own F6 marker"
        )
        assert app.state.streaming_text == "first half. "

        await gateway.send_all(
            [
                event("message.delta", {"text": "second half."}),
                event("message.complete", {"text": "first half. second half."}),
            ]
        )

        def _has_assistant_entry() -> bool:
            return any(entry.kind == "assistant" for entry in app.state.transcript)

        await settle(_has_assistant_entry)

        transcript_after_completion = app.state.transcript
        assistant_text: list[str] = [
            entry.text for entry in transcript_after_completion if entry.kind == "assistant"
        ]
        assert assistant_text == ["first half. second half."], (
            f"the resumed response duplicated or lost content: {assistant_text}"
        )
        await app.shutdown_sources()
