"""R35, F6, AE8: what a dropped socket is allowed to claim, and what it must not.

Every test here runs the real :class:`~talaria.transport.source.LiveSource`
against the stub gateway over a real loopback socket, and most of them run it
through the real :class:`~talaria.ui.app.TalariaApp` as well. That is deliberate:
R35's clauses are about what the *interface* shows after a transport event, and
a transport-only test would prove the correlator behaved while saying nothing
about whether the operator was told the truth.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path
from typing import Any, get_args

import pytest

from talaria.domain.models import ConnectionStatus
from talaria.recorder.framelog import FrameRecorder
from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential, CredentialError, LoopbackTokenProvider
from talaria.transport.rpc import LOST_WITH_TRANSPORT
from talaria.transport.source import LiveConnectionState, LiveSource
from talaria.ui.app import TalariaApp
from tests.transport.conftest import STUB_TOKEN, StubGateway, err, event, ok

FAST_RETRIES = (0.0, 0.01, 0.01)

#: One distinctive credential, swept for across every operator-facing surface.
CANARY = "canary-Ttbdq2Rk-do-not-leak"


class ScriptedProvider:
    """A credential provider double that counts dials and can rotate.

    KTD11's per-dial rule is invisible from outside a provider that happens to
    return the same string every time, so the count is the whole point.
    """

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


async def drain_into(app: TalariaApp, *, until: int, timeout: float = 5.0) -> None:
    """Wait until the app has folded at least ``until`` frames."""

    async def _poll() -> None:
        while app.frames_applied < until:
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


def live_app(source: LiveSource, **kwargs: Any) -> TalariaApp:
    app = TalariaApp(source, mode="live", dispatcher=source, **kwargs)
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)
    return app


async def settle(predicate: Any, *, timeout: float = 5.0) -> None:
    """Wait until ``predicate()`` is true, or fail the test on the timeout."""

    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


# ── the two enums must not drift ─────────────────────────────────────────


def test_the_transport_and_domain_connection_enums_are_identical() -> None:
    """The transport re-declares the enum rather than importing the domain.

    That is ADR-0002's boundary working as intended, and it is also exactly how
    two spellings of one contract drift apart. This test is the cost of the
    boundary, paid once.
    """
    assert set(get_args(LiveConnectionState)) == set(get_args(ConnectionStatus))


# ── R35: the four states are distinct and visible ────────────────────────


@pytest.mark.asyncio
async def test_a_successful_attach_walks_connecting_then_connected(
    gateway: StubGateway,
) -> None:
    seen: list[tuple[str, str]] = []
    source = live_source(
        gateway,
        on_connection=lambda state, detail, cause=None: seen.append((state, detail)),
    )

    assert await source.start() == "connected"
    await source.close()

    assert [state for state, _ in seen] == ["connecting", "connected", "disconnected"]
    assert source.correlator.epoch == 1


@pytest.mark.asyncio
async def test_a_rejected_credential_reaches_the_composer_as_auth_failed(
    gateway: StubGateway,
) -> None:
    """F1: the failure is named on screen, not shown as a generic disconnect."""
    source = live_source(gateway, ScriptedProvider(["the-wrong-token"]))
    app = live_app(source)

    async with app.run_test():
        await app.drain(timeout=10)
        assert source.state == "auth_failed"
        assert source.failure_kind == "auth_failed"
        assert app.state.connection == "auth_failed"
        assert "authentication failed" in app.composer.notice
        await app.shutdown_sources()

    assert gateway.rejections == 1
    assert source.reconnects == 0


@pytest.mark.asyncio
async def test_an_unreachable_gateway_is_distinguishable_from_a_rejected_one(
    gateway: StubGateway,
) -> None:
    """R35 keeps 'initial connection failure' apart from 'authentication failure'.

    KTD5's status enum has no ``connect_failed`` member and is frozen, so the
    distinction lives in ``failure_kind`` plus a cause shown beside the state —
    not in a sixth enum value that would be a ``version: 2`` change.
    """
    await gateway.stop()
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await app.drain(timeout=10)
        assert source.state == "disconnected"
        assert source.failure_kind == "connect_failed"
        assert app.state.connection == "disconnected"
        assert app.composer.notice != ""
        assert "authentication" not in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_drop_shows_reconnecting_and_then_connected_again(
    gateway: StubGateway,
) -> None:
    seen: list[str] = []
    source = live_source(gateway)
    app = live_app(source)

    def _observe(state: str, detail: str, cause: Any = None) -> None:
        seen.append(state)
        app.note_connection_state(state, detail, cause)  # type: ignore[arg-type]

    source.bind(on_connection=_observe)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.hang_up()
        await gateway.wait_for_attach()
        await drain_into(app, until=2)

        assert "reconnecting" in seen
        assert seen[-1] == "connected"
        assert source.reconnects == 1
        assert app.state.connection == "connected"
        await app.shutdown_sources()


@pytest.mark.parametrize(
    "endpoint", ["http://127.0.0.1:9119/api/ws", "ws:///api/ws"]
)
@pytest.mark.asyncio
async def test_a_dial_failure_shows_the_operator_no_credential(endpoint: str) -> None:
    """R9 carried all the way to the screen, for the two endpoint typos that leak.

    ``websockets`` raises ``InvalidURI`` whose text is the whole dialled URI, and
    the dialled URI is the only credentialed string in the system. The failure
    detail therefore travelled — unredacted — into ``LiveSource.last_failure``,
    into the ``on_connection`` detail, and out of ``note_connection_state`` into
    the composer notice, which is the one place a bystander reading over the
    operator's shoulder would see it.
    """
    source = LiveSource(
        AttachTarget.from_url(endpoint),
        ScriptedProvider([CANARY]),
        reconnect_delays=FAST_RETRIES,
    )
    details: list[str] = []
    app = live_app(source)

    def _observe(state: str, detail: str, cause: Any = None) -> None:
        details.append(detail)
        app.note_connection_state(state, detail, cause)  # type: ignore[arg-type]

    source.bind(on_connection=_observe)

    async with app.run_test():
        await app.drain(timeout=10)
        assert source.state == "disconnected"
        assert source.failure_kind == "connect_failed"
        assert CANARY not in source.last_failure, "the token reached last_failure"
        assert CANARY not in "\n".join(details), "the token reached the on_connection detail"
        assert CANARY not in app.composer.notice, "the token reached the composer"
        # The notice still says something: withholding must not mean silence.
        assert app.composer.notice.strip() != ""
        await app.shutdown_sources()


# ── a local credential problem is not a gateway rejection (T6) ───────────


class BrokenProvider:
    """A provider whose credential cannot be acquired at all."""

    def __init__(self, message: str) -> None:
        self.message = message
        self.calls = 0

    async def acquire(self) -> Credential:
        self.calls += 1
        raise CredentialError(self.message)


@pytest.mark.asyncio
async def test_a_local_credential_problem_is_not_reported_as_a_gateway_rejection(
    gateway: StubGateway,
) -> None:
    """No dial was made, so no gateway rejected anything.

    Every ``CredentialError`` — a credential file with the wrong mode, a
    malformed one, no source at all — used to become ``failure_kind
    "auth_failed"``, which the interface renders as "authentication failed — the
    gateway rejected the credential". For a local ``chmod`` problem that sends the
    operator to rotate a token no gateway ever saw. The branch had no test at all.
    """
    # The real refusal's opening words, since KTD8 retired the environment
    # variable this message used to advertise. Kept in step with
    # ``LoopbackTokenProvider._resolve`` deliberately: what this test proves is
    # that the *actionable* part of a credential refusal survives the trip to
    # the notice bar, and a stale stand-in would prove that about text no
    # operator ever sees.
    provider = BrokenProvider(
        "no gateway credential: run `talaria refresh-credential` to write "
        "/tmp/credentials at mode 0600"
    )
    source = live_source(gateway, provider)
    app = live_app(source)

    async with app.run_test():
        await app.drain(timeout=10)

        assert provider.calls >= 1
        assert source.failure_kind == "credential_unavailable"
        assert source.state == "disconnected"
        assert app.state.connection == "disconnected"
        # The two claims that were false: no dial happened, and the operator was
        # not told the gateway had an opinion.
        assert gateway.handshakes == [], "a dial was attempted with no credential"
        assert "rejected the credential" not in app.composer.notice
        assert "authentication failed" not in app.composer.notice
        # And the actionable part survives.
        assert "talaria refresh-credential" in app.composer.notice
        assert "HERMES_DASHBOARD_SESSION_TOKEN" not in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_credential_file_with_the_wrong_mode_stops_before_the_dial(
    gateway: StubGateway, tmp_path: Path
) -> None:
    """The same distinction, driven by the real provider rather than a double."""
    path = tmp_path / "credentials"
    path.write_text(f'token = "{STUB_TOKEN}"\n', encoding="utf-8")
    path.chmod(0o644)

    source = live_source(
        gateway, LoopbackTokenProvider(credentials_path=path, allow_prompt=False)
    )
    assert await source.start() == "disconnected"
    assert source.failure_kind == "credential_unavailable"
    assert "0644" in source.last_failure
    assert gateway.handshakes == []
    await source.close()


# ── KTD11: the credential is acquired per dial ───────────────────────────


@pytest.mark.asyncio
async def test_the_provider_is_invoked_once_per_dial_and_again_on_reconnect(
    gateway: StubGateway,
) -> None:
    provider = ScriptedProvider([STUB_TOKEN])
    source = live_source(gateway, provider)

    await source.start()
    assert provider.calls == 1

    await gateway.hang_up()
    await gateway.wait_for_attach()
    await asyncio.sleep(0.05)

    assert provider.calls == 2, "the credential was cached across a reconnect"
    assert source.reconnects == 1
    await source.close()


@pytest.mark.asyncio
async def test_a_credential_rotated_between_attach_and_reconnect_is_picked_up(
    gateway: StubGateway,
) -> None:
    """The reason the provider is a seam at all (KTD11): the value can change.

    The stub rotates with it, so a client that reused the first credential would
    be rejected — the assertion is that the reconnect *succeeded*, which only a
    re-read can produce.
    """
    provider = ScriptedProvider(["first-token", "rotated-token"])
    gateway.token = "first-token"
    source = live_source(gateway, provider)

    assert await source.start() == "connected"
    gateway.token = "rotated-token"
    await gateway.hang_up()
    await gateway.wait_for_attach()
    await asyncio.sleep(0.05)

    assert source.state == "connected"
    assert source.reconnects == 1
    assert gateway.queries[-1] == {"token": "rotated-token"}
    assert gateway.rejections == 0
    await source.close()


@pytest.mark.asyncio
async def test_a_credential_that_stops_working_stops_the_reconnect_loop(
    gateway: StubGateway,
) -> None:
    """A rejected credential does not become acceptable by being retried.

    The refusal here is the *handshake* shape. The post-accept close shape is
    exercised by the next test, because which of the two a real Hermes produces
    is decided by its ASGI server and this repository has not measured it.
    """
    provider = ScriptedProvider([STUB_TOKEN, "no-longer-valid"])
    source = live_source(gateway, provider)

    assert await source.start() == "connected"
    await gateway.hang_up()

    async def _settle() -> None:
        while source.state not in ("auth_failed", "disconnected"):
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_settle(), timeout=5)
    assert source.state == "auth_failed"
    assert source.reconnects == 0
    # Bounded: one rejected attempt, not one per retry slot.
    assert gateway.rejections == 1
    await source.close()


@pytest.mark.asyncio
async def test_an_authentication_close_after_accept_also_stops_the_loop() -> None:
    """The other shape a pre-accept ``ws.close(4401)`` can reach a client as.

    Here the handshake completes and the refusal arrives as a close code on the
    first read, so it surfaces through the *disconnect* path rather than the
    dial path. Without the classification there, a rejected credential would
    drive the reconnect loop to exhaustion and then report ``disconnected`` —
    sending the operator to check whether the gateway is running.
    """
    stub = StubGateway(reject_with="close")
    await stub.start()
    try:
        source = LiveSource(
            AttachTarget.from_url(stub.url),
            ScriptedProvider(["the-wrong-token"]),
            reconnect_delays=FAST_RETRIES,
        )
        assert await source.start() == "connected", "the handshake was expected to complete"

        async def _settle() -> None:
            while source.state == "connected":
                await asyncio.sleep(0.005)

        await asyncio.wait_for(_settle(), timeout=5)
        assert source.state == "auth_failed"
        assert source.failure_kind == "auth_failed"
        assert source.reconnects == 0
        await source.close()
        assert source.state == "auth_failed", "close() erased the reason"
    finally:
        await stub.stop()


# ── AE8: an interrupted call is unknown, and the UI says so ──────────────


@pytest.mark.asyncio
async def test_a_socket_drop_mid_rpc_resolves_the_call_to_unknown(
    gateway: StubGateway,
) -> None:
    """The stub deliberately never answers ``prompt.submit``."""
    source = live_source(gateway)
    await source.start()

    call = asyncio.ensure_future(source.call("prompt.submit", {"text": "hello"}))
    await gateway.wait_for_request("prompt.submit")
    await gateway.hang_up()

    outcome = await asyncio.wait_for(call, timeout=5)
    assert outcome.status == "unknown"
    assert outcome.confirmed is False
    assert source.correlator.lost_outcomes == 1
    await source.close()


@pytest.mark.asyncio
async def test_in_flight_calls_are_abandoned_before_the_re_dial(
    gateway: StubGateway,
) -> None:
    """The ordering inside ``_handle_disconnect``, pinned where it is observable.

    Abandoning after a successful re-dial leaves a window in which epoch 2 is
    open while futures from epoch 1 are still outstanding — and because ids
    restart per epoch, a reply in that window can satisfy the wrong call. The
    window is not directly observable from outside, so this reads the correlator
    from *inside* the ``connected`` callback of the new epoch, which is the first
    instant the window exists. Anything still in flight there is in the window.

    Moving the abandon after the re-dial passed the whole suite before this
    existed.
    """
    seen: list[tuple[str, int, int, int]] = []

    def _observe(state: str, detail: str, cause: Any = None) -> None:
        seen.append(
            (
                state,
                source.correlator.epoch,
                source.correlator.in_flight,
                source.correlator.lost_outcomes,
            )
        )

    source = live_source(gateway, on_connection=_observe)
    await source.start()

    call = asyncio.ensure_future(source.call("prompt.submit", {"text": "hi"}))
    await gateway.wait_for_request("prompt.submit")
    assert source.correlator.in_flight == 1

    await gateway.hang_up()
    await gateway.wait_for_attach()
    # The server accepts before the client's dial returns, so wait for the client
    # to have observed its own reconnect rather than for the handshake.
    await settle(lambda: source.reconnects == 1)
    assert (await asyncio.wait_for(call, timeout=5)).status == "unknown"

    connects = [row for row in seen if row[0] == "connected"]
    assert len(connects) == 2, seen
    assert connects[1] == ("connected", 2, 0, 1), (
        "epoch 2 was opened while an epoch-1 call was still outstanding — "
        f"the window a reused id can be answered in: {connects}"
    )
    await source.close()


@pytest.mark.asyncio
async def test_a_reply_from_a_dead_epoch_cannot_resolve_a_reused_identifier(
    gateway: StubGateway,
) -> None:
    """KTD13's epoch guard, exercised through the live reader for the first time.

    The reader used to hand the correlator ``correlator.epoch`` — the correlator's
    own current value — so ``resolve``'s ``epoch != self._epoch`` compared a
    number with itself and could never be true. The guard was dead in production
    and ``stale_epoch_replies`` could only move in a hand-written unit test.

    **The reconnect here is driven by hand, and that is the honest part.** In the
    current single-reader design nothing between ``recv()`` returning a frame and
    that frame being correlated can open a new epoch, so the interleaving cannot
    be produced by hanging the socket up. What can be produced — and what is
    produced here — is the *state* the guard exists for: the correlator moved on
    to epoch 2 and minted the reused id, while the reader is still pulling frames
    from the epoch-1 socket. The three calls below are exactly what
    ``_handle_disconnect`` and ``call`` do, in their order. If the reader asks the
    correlator what epoch it is on, the late reply resolves the wrong call.
    """
    source = live_source(gateway)
    await source.start()
    assert source.epoch == 1

    first = asyncio.ensure_future(source.call("prompt.submit", {"text": "first"}))
    sent = await gateway.wait_for_request("prompt.submit")
    frames_before = source.frames_received

    source.correlator.abandon(1, LOST_WITH_TRANSPORT)
    assert (await asyncio.wait_for(first, timeout=5)).status == "unknown"

    source.correlator.open_epoch()
    reused, second_future = source.correlator.begin("prompt.submit", {"text": "second"})
    assert reused.id == sent["id"], "ids must be reusable, or this race is unreachable"
    assert reused.epoch == 2

    # The dead epoch's answer, arriving on the socket it was asked on.
    await gateway.send(ok(sent["id"], {"queued": True}))
    await settle(lambda: source.frames_received > frames_before)

    assert source.correlator.stale_epoch_replies == 1, (
        "the reply was correlated against the correlator's own epoch, not the "
        "socket's — the guard cannot fire from any live path"
    )
    assert not second_future.done(), "a stale reply resolved the reused identifier"

    await source.close()
    assert (await second_future).status == "unknown"


@pytest.mark.asyncio
async def test_the_transcript_marks_an_unconfirmed_submit_rather_than_a_sent_one(
    gateway: StubGateway,
) -> None:
    """AE8's UI half: 'the UI shows it as such, not success'."""
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        call = asyncio.ensure_future(app.submit_live("please do the thing"))
        await gateway.wait_for_request("prompt.submit")
        await gateway.hang_up()
        outcome = await asyncio.wait_for(call, timeout=5)

        assert outcome is not None and outcome.status == "unknown"
        kinds = [entry.kind for entry in app.state.transcript]
        texts = [entry.text for entry in app.state.transcript]
        assert "user" in kinds
        assert "please do the thing" in texts
        assert any("delivery unconfirmed" in text for text in texts), (
            "an unconfirmed submit rendered as an ordinary delivered message"
        )
        assert "unknown" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_confirmed_submit_writes_the_line_and_clears_the_composer(
    gateway: StubGateway,
) -> None:
    gateway.responder = lambda message, stub: ok(message["id"], {"queued": True})
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        app.composer.text = "hello there"
        outcome = await asyncio.wait_for(app.submit_live(app.composer.text), timeout=5)

        assert outcome is not None and outcome.confirmed
        assert app.composer.text == ""
        assert [entry.text for entry in app.state.transcript if entry.kind == "user"] == [
            "hello there"
        ]
        assert not any(
            "unconfirmed" in entry.text for entry in app.state.transcript
        )
        request = gateway.current.received[-1]
        assert request["method"] == "prompt.submit"
        assert request["params"]["text"] == "hello there"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_refused_submit_keeps_the_text_and_writes_nothing(
    gateway: StubGateway,
) -> None:
    """A message the gateway rejected was not said, so nothing is transcribed."""
    gateway.responder = lambda message, stub: err(
        message["id"], 4090, "too many active sessions"
    )
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        outcome = await asyncio.wait_for(app.submit_live("hello"), timeout=5)

        assert outcome is not None and outcome.status == "error"
        assert [entry for entry in app.state.transcript if entry.kind == "user"] == []
        assert "too many active sessions" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_interrupt_only_cancels_the_turn_when_the_gateway_agrees(
    gateway: StubGateway,
) -> None:
    """R4 plus AE8: an unconfirmed interrupt must not apply the sticky state.

    ``cancelled`` suppresses later deltas, so applying it on an ``unknown``
    would silently swallow the rest of a turn that never stopped.
    """
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [event("message.start", {}), event("message.delta", {"text": "working"})]
        )
        await drain_into(app, until=3)
        assert app.state.turn == "streaming"

        call = asyncio.ensure_future(app.interrupt_live())
        await gateway.wait_for_request("session.interrupt")
        await gateway.hang_up()
        outcome = await asyncio.wait_for(call, timeout=5)

        assert outcome is not None and outcome.status == "unknown"
        assert app.state.turn == "streaming", "an unknown interrupt claimed the turn stopped"
        assert any("unknown" in entry.text for entry in app.state.transcript)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_confirmed_interrupt_marks_the_turn_cancelled(
    gateway: StubGateway,
) -> None:
    gateway.responder = lambda message, stub: ok(message["id"], {"status": "interrupted"})
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [event("message.start", {}), event("message.delta", {"text": "working"})]
        )
        await drain_into(app, until=3)

        outcome = await asyncio.wait_for(app.interrupt_live(), timeout=5)
        assert outcome is not None and outcome.confirmed
        assert app.state.turn == "cancelled"
        assert any("[interrupted]" in entry.text for entry in app.state.transcript)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_call_made_while_disconnected_is_unknown_not_an_exception(
    gateway: StubGateway,
) -> None:
    source = live_source(gateway)
    await source.start()
    await source.close()

    outcome = await source.call("prompt.submit", {"text": "hi"})
    assert outcome.status == "unknown"
    assert "not connected" in outcome.notice


@pytest.mark.asyncio
async def test_a_call_with_no_reply_times_out_as_unknown(gateway: StubGateway) -> None:
    source = live_source(gateway)
    await source.start()
    outcome = await source.call("prompt.submit", {"text": "hi"}, timeout=0.05)
    assert outcome.status == "unknown"
    assert "deadline" in outcome.notice
    await source.close()


# ── F6: reconnect reconciles without duplicating ─────────────────────────


@pytest.mark.asyncio
async def test_reconnect_replays_no_duplicate_transcript_entries(
    gateway: StubGateway,
) -> None:
    """F6, stated exactly: *the reconnect itself* adds nothing to the transcript.

    What makes this true is what Talaria does **not** do — it does not reset the
    focused session and it does not re-request history. The tempting alternative
    (clear, re-read, re-render) is precisely how a reconnect duplicates a
    transcript, and nothing in the domain would catch it, because every entry
    would be a legitimately new append.
    """
    source = live_source(gateway)
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [
                event("message.start", {}),
                event("message.delta", {"text": "half a sentence"}),
                event("message.complete", {"text": "half a sentence and the rest"}),
            ]
        )
        await drain_into(app, until=4)
        before = tuple(app.state.transcript)
        assert before, "the fixture streamed nothing to duplicate"

        await gateway.hang_up()
        await gateway.wait_for_attach()
        await drain_into(app, until=5)

        after = tuple(app.state.transcript)
        assert after[: len(before)] == before, "reconnect rewrote settled history"
        added = [entry.text for entry in after[len(before) :]]
        assert added == ["reconnected to the gateway"], (
            f"reconnect added transcript content beyond its own marker: {added}"
        )
        assert app.state.focused_session_id == "s1", "reconnect dropped the focused session"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_outstanding_prompt_re_announced_after_reconnect_is_keyed_once(
    gateway: StubGateway,
) -> None:
    """R8's registry is what makes re-delivery idempotent (F6).

    The gateway's blocking bridges outlive one socket — they expire on their own
    30-second timer (``tui_gateway/server.py:2981-2998``) — so an outstanding
    prompt being re-announced on a fresh connection is ordinary, not exotic.
    """
    source = live_source(gateway)
    app = live_app(source)
    prompt = event(
        "clarify.request", {"request_id": "req-1", "question": "which branch?"}
    )

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send(prompt)
        await drain_into(app, until=2)
        assert [p.request_id for p in app.state.prompts] == ["req-1"]

        await gateway.hang_up()
        await gateway.wait_for_attach()
        await drain_into(app, until=3)
        await gateway.send(prompt)
        await drain_into(app, until=4)

        assert [p.request_id for p in app.state.prompts] == ["req-1"]
        prompt_lines = [e for e in app.state.transcript if e.kind == "prompt"]
        assert len(prompt_lines) == 1, "the re-announced prompt was transcribed twice"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_sub_agent_re_announced_after_reconnect_keeps_its_terminal_state(
    gateway: StubGateway,
) -> None:
    """F6's sub-agent clause, and the clobber KTD8's guard exists to prevent."""
    source = live_source(gateway)
    app = live_app(source)
    start = event("subagent.start", {"subagent_id": "sa-1", "goal": "run the tests"})

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_all(
            [
                start,
                event("subagent.complete", {"subagent_id": "sa-1", "status": "completed"}),
            ]
        )
        await drain_into(app, until=3)
        assert [(r.id, r.status) for r in app.state.subagents] == [("sa-1", "completed")]

        await gateway.hang_up()
        await gateway.wait_for_attach()
        await drain_into(app, until=4)
        await gateway.send(start)
        await drain_into(app, until=5)

        assert [(r.id, r.status) for r in app.state.subagents] == [("sa-1", "completed")], (
            "a re-announced start duplicated the row or clobbered its terminal state"
        )
        await app.shutdown_sources()


# ── recording, both directions, through the U2 boundary ──────────────────


@pytest.mark.asyncio
async def test_both_directions_are_recorded_and_outbound_secrets_are_withheld(
    gateway: StubGateway, tmp_path: Path
) -> None:
    """R9/R26: the outbound half now exists, and it passes the same boundary."""
    log = tmp_path / "live.jsonl"
    target = AttachTarget.from_url(gateway.url)
    recorder = FrameRecorder(log, target.url)
    source = live_source(gateway, recorder=recorder)

    await source.start()
    await source.call("sudo.respond", {"password": "hunter2"}, timeout=0.05)
    await asyncio.sleep(0.05)
    await source.close()

    records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
    directions = [r.get("dir") for r in records if r["kind"] == "frame"]
    assert "in" in directions and "out" in directions

    outbound = [r for r in records if r.get("dir") == "out"]
    assert outbound, "nothing outbound reached the recorder"
    assert "hunter2" not in log.read_text(encoding="utf-8")
    assert outbound[0]["frame"]["params"]["password"] == "[redacted]"
    assert outbound[0]["redactions"][0]["reason"] == "deny-set:sudo.respond"


@pytest.mark.asyncio
async def test_an_unparseable_frame_becomes_a_withheld_hole_not_a_rendered_line(
    gateway: StubGateway, tmp_path: Path
) -> None:
    log = tmp_path / "live.jsonl"
    target = AttachTarget.from_url(gateway.url)
    source = live_source(gateway, recorder=FrameRecorder(log, target.url))
    app = live_app(source)

    async with app.run_test():
        await drain_into(app, until=1)
        await gateway.send_raw("{not json at all")
        await drain_into(app, until=2)

        texts = [entry.text for entry in app.state.transcript]
        assert any("could not be parsed" in text for text in texts)
        assert not any("not json at all" in text for text in texts)
        await app.shutdown_sources()

    assert "not json at all" not in log.read_text(encoding="utf-8")


# ── backpressure (KTD13's numbers) ───────────────────────────────────────


@pytest.mark.asyncio
async def test_reads_pause_at_the_frame_bound_and_resume_at_half(
    gateway: StubGateway,
) -> None:
    """AE16's missing number, made observable.

    The consumer here is the test itself, draining one frame at a time, which is
    the only way to hold the queue at a chosen depth. Bounds are lowered rather
    than the corpus enlarged: the rule under test is "pause at the bound", and
    proving it at 1,000 frames instead of 4 would cost seconds and prove the
    same thing.
    """
    source = live_source(gateway, max_queued_frames=4, max_queued_bytes=1 << 30)
    await source.start()

    iterator = source.__aiter__()
    await gateway.send_all([event("status.update", {"text": f"n{index}"}) for index in range(8)])

    async def _until_paused() -> None:
        while source.read_pauses == 0:
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_until_paused(), timeout=5)
    assert source.peak_queued_frames >= 4
    # Reads stopped, so the socket — not an unbounded queue — is holding the rest.
    assert source.frames_received <= 5

    for _ in range(4):
        await asyncio.wait_for(iterator.__anext__(), timeout=5)

    async def _until_resumed() -> None:
        while source.frames_received < 8:
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_until_resumed(), timeout=5)
    await source.close()


@pytest.mark.asyncio
async def test_reads_pause_at_the_byte_bound_and_resume_at_half(
    gateway: StubGateway,
) -> None:
    """The byte bound, asserted as a pause rather than as a peak.

    The previous version of this test asserted only ``peak_queued_bytes >= 512``,
    which the loop it had already waited on implies: ``_enqueue`` updates the peak
    *before* it increments ``read_pauses``, so waiting for a pause guarantees the
    peak. It therefore proved nothing about backpressure — deleting
    ``await self._reads_allowed.wait()`` from the read loop failed the frame-bound
    test and passed this one.

    What is asserted now is the behaviour itself: with six frames on the wire and
    a 512-byte bound, the socket still holds most of them after the reader has had
    time to drain everything, and they arrive only once the consumer has taken
    frames off the queue.
    """
    frames = 6
    source = live_source(gateway, max_queued_frames=10_000, max_queued_bytes=512)
    await source.start()
    iterator = source.__aiter__()

    await gateway.send_all(
        [event("status.update", {"text": "x" * 300}) for _ in range(frames)]
    )

    await settle(lambda: source.read_pauses > 0)
    # Long enough that an un-paused reader would have drained the whole burst off
    # loopback several times over. Without this the assertion below would be a
    # race rather than a bound.
    await asyncio.sleep(0.2)

    stalled = source.frames_received
    assert stalled < frames, (
        f"reads did not stop: {stalled} of {frames} frames were read while paused"
    )
    assert source.peak_queued_bytes >= 512

    # Draining below half the bound must let the rest through — a pause that never
    # resumes is a hang, not backpressure.
    for _ in range(frames):
        await asyncio.wait_for(iterator.__anext__(), timeout=5)

    assert source.frames_received == frames, "reads never resumed after the queue drained"
    await source.close()


# ── teardown (R36) ───────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_close_is_idempotent_and_leaves_nothing_pending(
    gateway: StubGateway,
) -> None:
    source = live_source(gateway)
    await source.start()
    call = asyncio.ensure_future(source.call("prompt.submit", {"text": "hi"}))
    await gateway.wait_for_request("prompt.submit")

    await source.close()
    await source.close()

    outcome = await asyncio.wait_for(call, timeout=5)
    assert outcome.status == "unknown"
    assert source.closed and source.state == "disconnected"


@pytest.mark.asyncio
async def test_closing_the_source_closes_the_socket_the_server_can_see(
    gateway: StubGateway,
) -> None:
    """R36's actual promise: the socket Talaria opened is shut, observed remotely.

    Nothing proved this. Replacing ``await connection.close()`` in
    ``_drop_connection`` with ``pass`` passed the whole suite, because every
    assertion about teardown read Talaria's own state — ``source.closed``,
    ``source.state`` — and Talaria's own state is exactly what a leaked socket
    would still report correctly. The only witness that cannot be fooled is the
    other end, so this one asks the server.
    """
    source = live_source(gateway)
    await source.start()
    session = await gateway.wait_for_attach()
    assert session.connection.close_code is None, "the fixture started already closed"

    await source.close()

    await asyncio.wait_for(session.connection.wait_closed(), timeout=5)
    assert session.connection.close_code is not None
    assert source.close_errors == 0


@pytest.mark.asyncio
async def test_closing_the_source_closes_the_recorder(
    gateway: StubGateway, tmp_path: Path
) -> None:
    """R36: the recorder is Talaria-owned, so teardown must finish its file."""
    log = tmp_path / "live.jsonl"
    target = AttachTarget.from_url(gateway.url)
    recorder = FrameRecorder(log, target.url)
    source = live_source(gateway, recorder=recorder)

    await source.start()
    await source.close()

    from talaria.recorder.framelog import RecorderError

    with pytest.raises(RecorderError, match="already closed"):
        recorder.record("in", "{}")
