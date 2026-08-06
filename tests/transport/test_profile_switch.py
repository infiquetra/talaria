"""U4's endpoint switch: what it drops, what it re-resolves, what it leaves behind.

Switching profile in Talaria means dialling a *different gateway* (KTD5), so
every test here stands up two stub gateways and asserts against both. The
credential is the thing under scrutiny: KTD6 requires it to be re-resolved for
the endpoint being dialled, because each profile's Hermes dashboard mints its
own token and carrying one across would present gateway A's credential to
gateway B — where the refusal reads as an authentication problem rather than as
"that credential was never for this gateway".

The three assertions that carry the weight:

* the credential handed to the second gateway came from a **fresh** provider,
  observed by the value that gateway saw on its own query string;
* a switch that fails leaves a **named** state and a report that says whether
  the previous connection is still there;
* nothing here, and nothing in the module under test, calls
  ``POST /api/profiles/active`` — see ``tests/transport/test_admin.py`` for the
  structural half of that assertion.

**No real profile name appears in this file (R12).** The two gateways are
addressed by URL, and where a name is needed it is a synthetic ``*-fixture``
one.
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest
import pytest_asyncio

from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential, CredentialError
from talaria.transport.source import (
    DISCONNECTING_SWITCH_REASONS,
    LiveSource,
    SwitchReport,
)
from tests.transport.conftest import STUB_TOKEN, StubGateway

FAST_RETRIES = (0.0, 0.01)

#: What the second gateway is configured to accept. Different from
#: :data:`STUB_TOKEN` on purpose: that difference is the whole of KTD6.
SECOND_TOKEN = "second-gateway-token-Wz7"


class PerEndpointProvider:
    """A credential provider bound to one endpoint, counting its own dials.

    Built by :func:`factory_for` — one instance per endpoint, never reused, so
    a test can assert that a switch produced a *new* provider rather than
    reusing the one already in hand.
    """

    def __init__(self, endpoint: str, value: str) -> None:
        self.endpoint = endpoint
        self.value = value
        self.acquisitions = 0

    async def acquire(self) -> Credential:
        self.acquisitions += 1
        if not self.value:
            raise CredentialError(
                "no gateway credential: run `talaria refresh-credential`"
            )
        return Credential("token", self.value, "file")


def factory_for(values: dict[str, str]) -> Any:
    """A credential factory over an endpoint-to-token map, recording its calls.

    ``values`` is keyed by the endpoint URL. An endpoint with no entry yields a
    provider that cannot produce a credential, which is the ordinary case for a
    profile whose dashboard has minted a token Talaria's single-value file does
    not hold.
    """
    built: list[PerEndpointProvider] = []

    def build(endpoint: str) -> PerEndpointProvider:
        provider = PerEndpointProvider(endpoint, values.get(endpoint, ""))
        built.append(provider)
        return provider

    build.built = built  # type: ignore[attr-defined]
    return build


async def settle(predicate: Any, *, timeout: float = 5.0) -> None:
    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


@pytest_asyncio.fixture
async def second_gateway() -> Any:
    """A second stub gateway accepting a *different* token from the first."""
    stub = StubGateway(token=SECOND_TOKEN)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


def source_for(gateway: StubGateway, factory: Any) -> LiveSource:
    return LiveSource(
        AttachTarget.from_url(gateway.url),
        factory(gateway.url),
        credential_factory=factory,
        reconnect_delays=FAST_RETRIES,
    )


# ── the happy path ────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_switch_dials_the_new_gateway_and_leaves_the_old_one(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    factory = factory_for({gateway.url: STUB_TOKEN, second_gateway.url: SECOND_TOKEN})
    source = source_for(gateway, factory)
    try:
        assert await source.start() == "connected"
        assert len(gateway.sessions) == 1

        report = await source.switch_to_endpoint(second_gateway.url)

        assert report.ok
        assert report.reason == "switched"
        assert report.state == "connected"
        assert source.switches == 1
        await settle(lambda: len(second_gateway.sessions) == 1)
        # The old gateway got no second session; the switch did not reconnect
        # to where it started.
        assert len(gateway.sessions) == 1
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_the_credential_is_re_resolved_for_the_endpoint_being_dialled(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    """KTD6, observed from the gateway's own side of the wire.

    The second gateway accepts only :data:`SECOND_TOKEN`. If the switch reused
    the provider already in hand it would present the first gateway's token and
    be refused — so a successful attach *is* the evidence that the credential
    was re-resolved for the endpoint.
    """
    factory = factory_for({gateway.url: STUB_TOKEN, second_gateway.url: SECOND_TOKEN})
    source = source_for(gateway, factory)
    try:
        await source.start()
        report = await source.switch_to_endpoint(second_gateway.url)
        assert report.ok

        await settle(lambda: len(second_gateway.handshakes) == 1)
        assert second_gateway.queries[-1]["token"] == SECOND_TOKEN
        assert gateway.queries[0]["token"] == STUB_TOKEN

        # A *new* provider was built for the new endpoint, and the old one was
        # not asked again — no value survived the switch.
        built = factory.built
        assert [p.endpoint for p in built] == [gateway.url, second_gateway.url]
        assert built[0].acquisitions == 1
        assert built[1].acquisitions == 1
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_a_switch_opens_a_new_epoch_and_announces_it(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    """KTD4's invalidation hook: the consumer must learn the epoch moved."""
    announced: list[int] = []
    factory = factory_for({gateway.url: STUB_TOKEN, second_gateway.url: SECOND_TOKEN})
    source = source_for(gateway, factory)
    source.bind(on_reconnect=announced.append)
    try:
        await source.start()
        first_epoch = source.epoch
        report = await source.switch_to_endpoint(second_gateway.url)
        assert report.ok
        assert source.epoch > first_epoch
        assert announced == [source.epoch]
    finally:
        await source.close()


# ── the failures, each leaving a named state ──────────────────────────────


@pytest.mark.asyncio
async def test_a_credential_the_new_endpoint_cannot_produce_is_named_and_not_dialled(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    """The common case (KTD6): nothing is dialled, so nothing refused anything."""
    factory = factory_for({gateway.url: STUB_TOKEN})  # second endpoint: no token
    source = source_for(gateway, factory)
    try:
        await source.start()
        report = await source.switch_to_endpoint(second_gateway.url)

        assert report.reason == "credential_unavailable"
        assert report.left_disconnected
        assert "refresh-credential" in report.detail
        # Named, not silent: the transport says where it is.
        assert report.state == "disconnected"
        assert source.state == "disconnected"
        assert source.failure_kind == "credential_unavailable"
        # No dial was made, so the second gateway never saw a handshake — the
        # distinction between "could not produce a credential" and "the
        # gateway refused one" is a real one on the wire.
        assert second_gateway.handshakes == []
        assert not source.closed
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_a_new_gateway_that_refuses_the_credential_is_auth_failed(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    factory = factory_for({gateway.url: STUB_TOKEN, second_gateway.url: "wrong-token"})
    source = source_for(gateway, factory)
    try:
        await source.start()
        report = await source.switch_to_endpoint(second_gateway.url)

        assert report.reason == "auth_failed"
        assert report.left_disconnected
        assert report.state == "auth_failed"
        # This one *was* dialled and *was* refused — the other side of the
        # distinction the previous test draws.
        assert second_gateway.rejections >= 1
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_a_gateway_that_is_not_listening_is_connect_failed(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    dead = second_gateway.url
    await second_gateway.stop()

    factory = factory_for({gateway.url: STUB_TOKEN, dead: SECOND_TOKEN})
    source = source_for(gateway, factory)
    try:
        await source.start()
        report = await source.switch_to_endpoint(dead)

        assert report.reason == "connect_failed"
        assert report.left_disconnected
        assert report.state == "disconnected"
        assert report.detail
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_every_disconnecting_reason_reports_that_it_disconnected() -> None:
    """The classification is data, so a new reason forces a decision about it."""
    for reason in DISCONNECTING_SWITCH_REASONS:
        report = SwitchReport(reason, "disconnected", "why")  # type: ignore[arg-type]
        assert report.left_disconnected
        assert not report.ok
    for reason in ("refused_endpoint", "unsupported", "closed"):
        report = SwitchReport(reason, "connected", "why")  # type: ignore[arg-type]
        assert not report.left_disconnected


# ── the refusals that touch nothing ───────────────────────────────────────


@pytest.mark.asyncio
async def test_an_unparseable_endpoint_is_refused_before_anything_is_dropped(
    gateway: StubGateway,
) -> None:
    factory = factory_for({gateway.url: STUB_TOKEN})
    source = source_for(gateway, factory)
    try:
        await source.start()
        report = await source.switch_to_endpoint("ws://[not-a-host")

        assert report.reason == "refused_endpoint"
        assert not report.left_disconnected
        # Still connected to where it was: this refusal costs the operator
        # nothing.
        assert source.connected
        assert source.state == "connected"
        # And the message never echoes the string that failed to parse.
        assert "not-a-host" not in report.detail
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_a_source_with_no_credential_factory_refuses_rather_than_reusing_one(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    """KTD6 cannot be honoured without one, so the switch does not happen."""
    factory = factory_for({gateway.url: STUB_TOKEN})
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        factory(gateway.url),
        reconnect_delays=FAST_RETRIES,
    )
    try:
        await source.start()
        report = await source.switch_to_endpoint(second_gateway.url)

        assert report.reason == "unsupported"
        assert not report.left_disconnected
        assert source.connected
        assert second_gateway.handshakes == []
    finally:
        await source.close()


@pytest.mark.asyncio
async def test_a_closed_source_is_not_reopened_by_a_switch(
    gateway: StubGateway, second_gateway: StubGateway
) -> None:
    factory = factory_for({gateway.url: STUB_TOKEN, second_gateway.url: SECOND_TOKEN})
    source = source_for(gateway, factory)
    await source.start()
    await source.close()

    report = await source.switch_to_endpoint(second_gateway.url)
    assert report.reason == "closed"
    assert second_gateway.handshakes == []


# ── the write that never happens (KTD5) ───────────────────────────────────


def test_the_transport_has_no_path_to_the_active_profile_write() -> None:
    """Talaria never calls ``POST /api/profiles/active`` (KTD5).

    Asserted here as well as in ``tests/transport/test_admin.py`` because this
    is the module that *would* be the tempting place to put it: a switch is
    exactly the moment an implementer reaches for "and tell the gateway".
    """
    from pathlib import Path

    from tests.transport.test_admin import _without_docstrings

    source = (Path(__file__).parents[2] / "talaria" / "transport" / "source.py").read_text(
        encoding="utf-8"
    )
    # Docstrings and comments are stripped first: the decision *not* to call
    # the POST is recorded in prose in this very module, and a check that
    # forbade documenting it would forbid explaining it. What is scanned is the
    # code, which is where a call would actually live.
    assert "profiles/active" not in _without_docstrings(source)
    assert not [name for name in dir(LiveSource) if "active_profile" in name.lower()]
