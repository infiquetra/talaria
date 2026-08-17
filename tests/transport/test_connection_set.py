"""U2's fleet transport: N gateways at once, and what one of them dying costs.

Every test here stands up **real** loopback stub gateways — the same
``StubGateway`` the single-connection transport tests dial — because the claims
under scrutiny are about sockets: that two of them are open at the same time,
that one hanging up leaves the other's traffic flowing, and that a second
profile's connection is *added* rather than swapped in over the first.

The three properties this file exists to pin:

* **Isolation.** One gateway going away marks its own connection, ends its own
  pump, and touches no other connection's epoch bookkeeping or frame flow.
* **Credential separation.** Each connection resolves its own
  ``[profiles.<name>]`` entry; a profile with no entry fails by name and never
  borrows another's; two profiles claiming one gateway with two different
  credentials refuse the launch and name both.
* **Ensure, not switch.** Selecting a second profile leaves the first
  connection's socket up — asserted by a subsequent call on it succeeding
  without a redial, which is the only observation that distinguishes "still
  connected" from "reconnected quickly".

**No real profile name appears in this file (R12).** Profiles are addressed by
synthetic ``*-fixture`` names and gateways by URL.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
from pathlib import Path
from typing import Any

import pytest
import pytest_asyncio

from talaria.transport.connection_set import (
    ConnectionConflictError,
    ConnectionEntry,
    ConnectionSet,
    PlannedConnection,
    TaggedFrame,
    build_source_factory,
    credential_provider_factory,
    plan_connections,
    resolve_connections,
)
from talaria.transport.credentials import DEFAULT_PROFILE_NAME, LoopbackTokenProvider
from tests.transport.conftest import STUB_TOKEN, StubGateway, ok

#: What the second stub gateway accepts. Different from :data:`STUB_TOKEN` on
#: purpose: each profile's dashboard mints its own token, and a fleet that
#: carried one across would present profile A's credential to profile B.
SECOND_TOKEN = "second-fleet-token-Q4m"

ALPHA = "alpha-fixture"
BETA = "beta-fixture"


@pytest_asyncio.fixture
async def two_gateways() -> Any:
    """Two independent stub gateways, each with its own accepted token."""
    first = StubGateway(responder=_echo_responder)
    second = StubGateway(token=SECOND_TOKEN, responder=_echo_responder)
    await first.start()
    await second.start()
    try:
        yield first, second
    finally:
        await first.stop()
        await second.stop()


def _echo_responder(message: dict[str, Any], _stub: StubGateway) -> dict[str, Any] | None:
    """Answer any JSON-RPC request with a result naming the method."""
    if "id" not in message:
        return None
    return ok(message["id"], {"echoed": message.get("method", "")})


def _write_credentials(
    path: Path, *, token: str = "", profiles: dict[str, str] | None = None
) -> Path:
    """A 0600 credential file with a default entry and/or per-profile entries."""
    lines = []
    if token:
        lines.append(f'token = "{token}"')
    for name, value in (profiles or {}).items():
        lines.append("")
        lines.append(f"[profiles.{name}]")
        lines.append(f'token = "{value}"')
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    path.chmod(0o600)
    return path


def _entries_for(first: StubGateway, second: StubGateway) -> tuple[ConnectionEntry, ...]:
    return (
        ConnectionEntry(profile=ALPHA, endpoint=first.url, credential_profile=ALPHA),
        ConnectionEntry(profile=BETA, endpoint=second.url, credential_profile=BETA),
    )


def _set_for(
    entries: tuple[ConnectionEntry, ...], credentials: Path
) -> ConnectionSet:
    return ConnectionSet(
        entries,
        build_source_factory(provider_for=credential_provider_factory(credentials)),
    )


# ── the inventory ────────────────────────────────────────────────────────


def test_no_config_leaves_the_inventory_as_the_single_credential_file_gateway() -> None:
    """KTD1's floor: with nothing configured, v0.3's one connection is the fleet."""
    plan = plan_connections(default_endpoint="ws://127.0.0.1:9119/api/ws")

    assert [member.name for member in plan] == [DEFAULT_PROFILE_NAME]
    # Named, not ``None``. This pinned ``None`` on the first draft, which is
    # what let ``refresh-credential --profile default`` write an entry the
    # default connection never read: a ``None`` provider skips the profile
    # level entirely, so ``[profiles.default]`` was invisible to the dial while
    # the CLI happily wrote it. Naming the profile is what makes KTD5's one
    # documented fallback — this table, then the top-level pair — actually run.
    assert plan[0].credential_profile == DEFAULT_PROFILE_NAME


def test_configured_endpoints_extend_the_inventory_in_config_order() -> None:
    plan = plan_connections(
        default_endpoint="ws://127.0.0.1:9119/api/ws",
        config_endpoints={ALPHA: "ws://127.0.0.1:9120/api/ws", BETA: "ws://127.0.0.1:9121/api/ws"},
    )

    assert [member.name for member in plan] == [DEFAULT_PROFILE_NAME, ALPHA, BETA]
    assert [member.credential_profile for member in plan] == [DEFAULT_PROFILE_NAME, ALPHA, BETA]


def test_a_configured_endpoint_carrying_a_credential_is_stripped_not_dialled_with_one() -> None:
    """The endpoint is normalized through the same guard the dial path uses."""
    plan = plan_connections(
        default_endpoint="ws://127.0.0.1:9119/api/ws",
        config_endpoints={ALPHA: "ws://127.0.0.1:9120/api/ws?token=NOT-A-REAL-TOKEN-canary"},
    )

    assert "NOT-A-REAL-TOKEN-canary" not in plan[1].endpoint


def test_an_unparseable_endpoint_stays_named_in_the_inventory() -> None:
    """R10: a configured profile Talaria cannot dial is named, never dropped."""
    plan = plan_connections(
        default_endpoint="ws://127.0.0.1:9119/api/ws",
        config_endpoints={ALPHA: "ws://[bad::/api/ws"},
    )

    assert plan[1].name == ALPHA
    assert plan[1].problem
    assert not plan[1].dialable_endpoint
    assert "bad" not in plan[1].problem


@pytest.mark.asyncio
async def test_two_rows_parsing_to_one_url_with_one_credential_produce_one_connection(
    tmp_path: Path,
) -> None:
    """KTD5: same gateway, byte-identical tokens — one connection, both names."""
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: "same-token-9x", BETA: "same-token-9x"}
    )
    plan = plan_connections(
        default_endpoint="ws://127.0.0.1:9119/api/ws",
        config_endpoints={
            ALPHA: "ws://127.0.0.1:9120/api/ws",
            BETA: "ws://127.0.0.1:9120/api/ws",
        },
    )

    entries = await resolve_connections(plan, credential_provider_factory(credentials))

    shared = [entry for entry in entries if entry.profile == ALPHA]
    assert len(shared) == 1
    assert shared[0].aliases == (BETA,)
    assert BETA not in [entry.profile for entry in entries]
    assert shared[0].names == (ALPHA, BETA)


@pytest.mark.asyncio
async def test_one_url_with_two_different_credentials_refuses_and_names_both(
    tmp_path: Path,
) -> None:
    """The refusal is loud, names both profiles, and never carries either token."""
    credentials = _write_credentials(
        tmp_path / "credentials",
        profiles={ALPHA: "token-for-alpha-A1", BETA: "token-for-beta-B2"},
    )
    plan = plan_connections(
        default_endpoint="ws://127.0.0.1:9119/api/ws",
        config_endpoints={
            ALPHA: "ws://127.0.0.1:9120/api/ws",
            BETA: "ws://127.0.0.1:9120/api/ws",
        },
    )

    with pytest.raises(ConnectionConflictError) as caught:
        await resolve_connections(plan, credential_provider_factory(credentials))

    message = str(caught.value)
    assert ALPHA in message
    assert BETA in message
    assert "token-for-alpha-A1" not in message
    assert "token-for-beta-B2" not in message


@pytest.mark.asyncio
async def test_a_profile_with_no_credential_never_folds_into_one_that_has_one(
    tmp_path: Path,
) -> None:
    """Sharing a URL is not sharing a credential; an unpaired profile stays its own.

    It must not silently become an alias of the paired one — that is exactly
    "borrowing another profile's token" wearing a different hat.
    """
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: "token-alpha"})
    plan = plan_connections(
        default_endpoint="ws://127.0.0.1:9119/api/ws",
        config_endpoints={
            ALPHA: "ws://127.0.0.1:9120/api/ws",
            BETA: "ws://127.0.0.1:9120/api/ws",
        },
    )

    entries = await resolve_connections(plan, credential_provider_factory(credentials))

    by_name = {entry.profile: entry for entry in entries}
    assert by_name[ALPHA].aliases == ()
    assert by_name[BETA].credential_profile == BETA


# ── credential resolution ────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_each_connection_dials_with_its_own_profile_entry(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """The token each gateway saw on its own query string is its own profile's."""
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    fleet = _set_for(_entries_for(first, second), credentials)

    reports = await fleet.start()
    try:
        assert [report.reason for report in reports] == ["connected", "connected"]
        assert first.queries[-1]["token"] == STUB_TOKEN
        assert second.queries[-1]["token"] == SECOND_TOKEN
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_a_missing_profile_entry_fails_by_name_and_borrows_nothing(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """KTD5's loud refusal: the unpaired profile is named and never dialled.

    The gateway it would have dialled records **no** upgrade attempt at all,
    which is the observation that proves no other profile's credential was
    tried against it.
    """
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", token=SECOND_TOKEN, profiles={ALPHA: STUB_TOKEN}
    )
    fleet = _set_for(_entries_for(first, second), credentials)

    reports = await fleet.start()
    try:
        by_profile = {report.profile: report for report in reports}
        assert by_profile[ALPHA].reason == "connected"
        assert by_profile[BETA].reason == "credential_unavailable"
        assert BETA in by_profile[BETA].detail
        assert "refresh-credential --profile" in by_profile[BETA].detail
        assert second.handshakes == []
        # And the paired connection is unaffected by its neighbour's failure.
        assert fleet.connected_profiles == (ALPHA,)
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_a_mode_0644_credential_file_is_refused_exactly_as_it_is_today(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """The file gate belongs to the file, not to one of its keys or entries."""
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    credentials.chmod(0o644)
    fleet = _set_for(_entries_for(first, second), credentials)

    reports = await fleet.start()
    try:
        assert {report.reason for report in reports} == {"credential_unavailable"}
        assert all("chmod 600" in report.detail for report in reports)
        assert first.handshakes == []
        assert second.handshakes == []
    finally:
        await fleet.close()


# ── isolation ────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_one_gateway_dying_leaves_the_others_traffic_flowing(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """The claim U2 exists for, measured on two live sockets.

    One gateway is stopped outright — not merely hung up, so its connection's
    reconnect schedule runs out and its stream genuinely ends — and the other
    connection is then asked to carry a request and answer it.
    """
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    entries = _entries_for(first, second)
    fleet = ConnectionSet(
        entries,
        build_source_factory(provider_for=credential_provider_factory(credentials)),
    )
    seen: list[TaggedFrame] = []

    async def drain() -> None:
        async for tagged in fleet:
            seen.append(tagged)

    await fleet.start()
    consumer = asyncio.create_task(drain())
    try:
        alpha = fleet.source_for(ALPHA)
        beta = fleet.source_for(BETA)
        assert alpha is not None and beta is not None
        beta._reconnect_delays = (0.0,)  # noqa: SLF001 - the test wants a fast give-up

        alpha_epoch_before = alpha.epoch
        await second.stop()
        for _ in range(200):
            if beta.state in {"disconnected", "auth_failed"}:
                break
            await asyncio.sleep(0.01)

        # The dead connection is named, and nothing about the live one moved.
        beta_status = fleet.status_of(BETA)
        assert beta_status is not None
        assert beta_status.state in {"disconnected", "auth_failed"}
        assert alpha.epoch == alpha_epoch_before
        assert alpha.state == "connected"

        outcome = await alpha.call("session.list", timeout=5.0)
        assert outcome.confirmed, outcome.status
        assert fleet.connected_profiles == (ALPHA,)

        await first.send(
            {
                "jsonrpc": "2.0",
                "method": "event",
                "params": {"type": "gateway.ready", "payload": {}},
            }
        )
        for _ in range(200):
            if any(frame.profile == ALPHA for frame in seen):
                break
            await asyncio.sleep(0.01)
        assert any(frame.profile == ALPHA for frame in seen)
        assert not consumer.done(), "one connection ending must not end the merged stream"
    finally:
        await fleet.close()
        # ``close`` ends the merged stream, so the drain task returns on its
        # own; the cancel is only for the paths where it did not.
        consumer.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await consumer


@pytest.mark.asyncio
async def test_every_frame_leaves_the_seam_carrying_its_connection_identity(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """U3 never has to guess a frame's origin, because U2 already stamped it."""
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    fleet = _set_for(_entries_for(first, second), credentials)
    seen: list[TaggedFrame] = []

    async def drain() -> None:
        async for tagged in fleet:
            seen.append(tagged)

    await fleet.start()
    consumer = asyncio.create_task(drain())
    try:
        for _ in range(200):
            if {frame.profile for frame in seen} == {ALPHA, BETA}:
                break
            await asyncio.sleep(0.01)
        assert {frame.profile for frame in seen} == {ALPHA, BETA}
        assert all(frame.epoch >= 1 for frame in seen)
    finally:
        await fleet.close()
        # ``close`` ends the merged stream, so the drain task returns on its
        # own; the cancel is only for the paths where it did not.
        consumer.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await consumer


# ── ensure, not switch ───────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_selecting_a_second_profile_leaves_the_first_socket_up(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """The whole of "``/profiles`` ensures instead of drop-switching".

    The first gateway records **one** accepted session for the whole test: a
    second one would mean the connection had been dropped and redialled, which
    is precisely what the drop-switch did and what this replaces. And a call
    issued on the first connection *after* the second was ensured still lands.
    """
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    entries = _entries_for(first, second)
    fleet = ConnectionSet(
        (entries[0],) + (entries[1],),
        build_source_factory(provider_for=credential_provider_factory(credentials)),
    )

    try:
        alpha_report = await fleet.ensure(ALPHA)
        assert alpha_report.ok
        assert len(first.sessions) == 1

        beta_report = await fleet.ensure(BETA)
        assert beta_report.reason == "connected"
        assert fleet.home == BETA

        alpha = fleet.source_for(ALPHA)
        assert alpha is not None
        assert alpha.state == "connected"
        assert len(first.sessions) == 1, "the first connection was dropped and redialled"

        outcome = await alpha.call("session.list", timeout=5.0)
        assert outcome.confirmed, outcome.status
        assert fleet.connected_profiles == (ALPHA, BETA)
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_ensuring_a_connection_already_up_costs_no_redial(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    fleet = _set_for(_entries_for(first, second), credentials)

    try:
        await fleet.ensure(ALPHA)
        again = await fleet.ensure(ALPHA)
        assert again.reason == "already_up"
        assert len(first.sessions) == 1
        assert fleet.home == ALPHA
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_an_alias_addresses_the_shared_connection_it_folded_into(
    tmp_path: Path, gateway: StubGateway
) -> None:
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: STUB_TOKEN}
    )
    plan = (
        PlannedConnection(name=ALPHA, endpoint=gateway.url, credential_profile=ALPHA),
        PlannedConnection(name=BETA, endpoint=gateway.url, credential_profile=BETA),
    )
    entries = await resolve_connections(plan, credential_provider_factory(credentials))
    fleet = ConnectionSet(
        entries,
        build_source_factory(provider_for=credential_provider_factory(credentials)),
    )

    try:
        report = await fleet.ensure(BETA)
        assert report.ok
        assert report.profile == ALPHA, "an alias resolves to the canonical identity"
        assert len(gateway.sessions) == 1
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_an_unknown_profile_is_refused_by_name_and_dials_nothing(
    tmp_path: Path, gateway: StubGateway
) -> None:
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN})
    fleet = _set_for(
        (ConnectionEntry(profile=ALPHA, endpoint=gateway.url, credential_profile=ALPHA),),
        credentials,
    )

    try:
        report = await fleet.ensure("never-configured-fixture")
        assert report.reason == "unknown_profile"
        assert not report.ok
        assert gateway.handshakes == []
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_a_gateway_that_is_not_listening_leaves_the_others_untouched(
    tmp_path: Path, gateway: StubGateway
) -> None:
    """Port 1 on loopback is reserved and nothing listens there."""
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    fleet = _set_for(
        (
            ConnectionEntry(profile=ALPHA, endpoint=gateway.url, credential_profile=ALPHA),
            ConnectionEntry(
                profile=BETA, endpoint="ws://127.0.0.1:1/api/ws", credential_profile=BETA
            ),
        ),
        credentials,
    )

    try:
        reports = await fleet.start()
        by_profile = {report.profile: report for report in reports}
        assert by_profile[ALPHA].reason == "connected"
        assert by_profile[BETA].reason == "connect_failed"
        assert fleet.connected_profiles == (ALPHA,)
        assert fleet.home == ALPHA
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_a_gateway_that_refuses_the_credential_is_auth_failed_alone(
    tmp_path: Path, gateway: StubGateway
) -> None:
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: "wrong-token-for-this-gateway"}
    )
    fleet = _set_for(
        (
            ConnectionEntry(profile=ALPHA, endpoint=gateway.url, credential_profile=ALPHA),
            ConnectionEntry(profile=BETA, endpoint=gateway.url, credential_profile=BETA),
        ),
        credentials,
    )

    try:
        reports = await fleet.start()
        by_profile = {report.profile: report for report in reports}
        assert by_profile[ALPHA].reason == "connected"
        assert by_profile[BETA].reason == "auth_failed"
        assert fleet.connected_profiles == (ALPHA,)
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_a_closed_set_is_not_reopened_by_an_ensure(
    tmp_path: Path, gateway: StubGateway
) -> None:
    """R36: teardown is teardown; ``ensure`` is not a door back in."""
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN})
    fleet = _set_for(
        (ConnectionEntry(profile=ALPHA, endpoint=gateway.url, credential_profile=ALPHA),),
        credentials,
    )

    await fleet.start()
    await fleet.close()
    await fleet.close()  # idempotent

    report = await fleet.ensure(ALPHA)
    assert report.reason == "closed"
    assert len(gateway.sessions) == 1


@pytest.mark.asyncio
async def test_a_refused_endpoint_is_named_and_never_dialled(tmp_path: Path) -> None:
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN})
    fleet = _set_for(
        (
            ConnectionEntry(
                profile=ALPHA,
                endpoint="",
                credential_profile=ALPHA,
                problem="the gateway endpoint is not a valid URL",
            ),
        ),
        credentials,
    )

    report = await fleet.ensure(ALPHA)
    assert report.reason == "refused_endpoint"
    assert "not a valid URL" in report.detail


# ── the credential file's own shapes ─────────────────────────────────────


@pytest.mark.asyncio
async def test_the_legacy_single_entry_file_still_answers_for_the_default_profile(
    tmp_path: Path,
) -> None:
    """The v0.1 shape is the default profile's entry, not a legacy to migrate."""
    credentials = _write_credentials(tmp_path / "credentials", token=STUB_TOKEN)
    provider = LoopbackTokenProvider(credentials_path=credentials, allow_prompt=False)

    credential = await provider.acquire()

    assert credential.value == STUB_TOKEN
    assert credential.source == "file"


@pytest.mark.asyncio
async def test_a_per_profile_entry_is_re_read_on_every_dial(tmp_path: Path) -> None:
    """KTD11's per-dial rule reaches the per-profile entries too.

    A rotated token must be picked up by the next reconnect without restarting
    Talaria, and that guarantee is carried entirely by re-reading the file.
    """
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: "first-value"})
    provider = LoopbackTokenProvider(
        credentials_path=credentials, allow_prompt=False, profile=ALPHA
    )

    assert (await provider.acquire()).value == "first-value"
    _write_credentials(credentials, profiles={ALPHA: "rotated-value"})
    assert (await provider.acquire()).value == "rotated-value"
    assert provider.acquisitions == 2


@pytest.mark.asyncio
async def test_a_named_profile_never_falls_back_to_the_default_entry(tmp_path: Path) -> None:
    """The refusal that makes multi-profile safe rather than merely possible."""
    from talaria.transport.credentials import CredentialError

    credentials = _write_credentials(tmp_path / "credentials", token=STUB_TOKEN)
    provider = LoopbackTokenProvider(
        credentials_path=credentials, allow_prompt=False, profile=ALPHA
    )

    with pytest.raises(CredentialError) as caught:
        await provider.acquire()

    message = str(caught.value)
    assert ALPHA in message
    assert f"refresh-credential --profile {ALPHA}" in message
    assert STUB_TOKEN not in message


@pytest.mark.asyncio
async def test_a_named_profile_never_prompts_however_it_was_built(tmp_path: Path) -> None:
    """A hidden prompt under a running interface is indistinguishable from a hang."""
    from talaria.transport.credentials import CredentialError

    asked: list[str] = []

    def prompt(label: str) -> str:
        asked.append(label)
        return "typed-value"

    provider = LoopbackTokenProvider(
        credentials_path=tmp_path / "missing",
        prompt=prompt,
        allow_prompt=True,
        profile=ALPHA,
    )

    with pytest.raises(CredentialError):
        await provider.acquire()
    assert asked == []


@pytest.mark.asyncio
async def test_a_malformed_profile_table_refuses_that_profile_and_no_other(
    tmp_path: Path,
) -> None:
    """KTD5's semantic isolation: one bad table costs one profile, not the file."""
    from talaria.transport.credentials import CredentialError

    path = tmp_path / "credentials"
    path.write_text(
        f'[profiles.{ALPHA}]\ntoken = 7\n\n[profiles.{BETA}]\ntoken = "beta-ok"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    with pytest.raises(CredentialError) as caught:
        await LoopbackTokenProvider(
            credentials_path=path, allow_prompt=False, profile=ALPHA
        ).acquire()
    assert ALPHA in str(caught.value)

    beta = await LoopbackTokenProvider(
        credentials_path=path, allow_prompt=False, profile=BETA
    ).acquire()
    assert beta.value == "beta-ok"


@pytest.mark.asyncio
async def test_a_per_profile_url_is_refused_because_endpoints_have_one_source(
    tmp_path: Path,
) -> None:
    """KTD5: an address may come from config and from nowhere else."""
    from talaria.transport.credentials import CredentialError

    path = tmp_path / "credentials"
    path.write_text(
        f'[profiles.{ALPHA}]\ntoken = "t"\nurl = "ws://127.0.0.1:9999/api/ws"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    with pytest.raises(CredentialError) as caught:
        await LoopbackTokenProvider(
            credentials_path=path, allow_prompt=False, profile=ALPHA
        ).acquire()
    assert "[profiles.endpoints]" in str(caught.value)


@pytest.mark.asyncio
async def test_a_syntax_error_fails_the_whole_file_and_names_it(tmp_path: Path) -> None:
    """One TOML document has one parse; there is no per-entry syntax boundary."""
    from talaria.transport.credentials import CredentialError

    path = tmp_path / "credentials"
    path.write_text('token = "unterminated\n', encoding="utf-8")
    path.chmod(0o600)

    for profile in (None, ALPHA):
        with pytest.raises(CredentialError) as caught:
            await LoopbackTokenProvider(
                credentials_path=path, allow_prompt=False, profile=profile
            ).acquire()
        assert "not valid TOML" in str(caught.value)


@pytest.mark.asyncio
async def test_no_credential_value_is_ever_carried_into_a_report(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """R22 at the fleet seam: a canary token appears in no status or report."""
    first, second = two_gateways
    canary = "NOT-A-REAL-TOKEN-fleet-canary-8Zq"
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: canary}
    )
    fleet = _set_for(_entries_for(first, second), credentials)

    try:
        reports = await fleet.start()
        rendered = json.dumps(
            [
                [report.profile, report.reason, report.state, report.detail]
                for report in reports
            ]
        ) + json.dumps(
            [
                [status.profile, status.endpoint, status.detail, status.failure_kind]
                for status in fleet.statuses
            ]
        )
        assert canary not in rendered
        assert STUB_TOKEN not in rendered
    finally:
        await fleet.close()


# ── the fleet's recording (KTD6) ─────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_two_connection_run_records_one_tagged_log_with_no_credential_in_it(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """The recorder half of U2, wired end to end through real sockets.

    Three claims in one run, because they are only true together: the log is
    one file at version 2 with both connections in its header; every frame
    names the connection it crossed; and a distinctive credential pushed
    through **each** connection's blocking bridge is absent from the bytes on
    disk. The last is why the recorder view exists at all — tagging happens
    after redaction, never instead of it.
    """
    from talaria.recorder.framelog import (
        FRAME_LOG_VERSION_MULTI_CONNECTION,
        FrameRecorder,
    )
    from talaria.transport.connection_set import recorded_connections

    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    entries = _entries_for(first, second)
    log = tmp_path / "fleet.jsonl"
    recorder = FrameRecorder(
        log, entries[0].endpoint, connections=recorded_connections(entries)
    )
    fleet = ConnectionSet(
        entries,
        build_source_factory(
            provider_for=credential_provider_factory(credentials), recorder=recorder
        ),
    )
    canaries = {
        ALPHA: "NOT-A-REAL-SECRET-alpha-canary-7Ha",
        BETA: "NOT-A-REAL-SECRET-beta-canary-2Vt",
    }

    try:
        await fleet.start()
        for profile, canary in canaries.items():
            source = fleet.source_for(profile)
            assert source is not None
            await source.call(
                "sudo.respond", {"request_id": "r-1", "password": canary}, timeout=5.0
            )
    finally:
        await fleet.close()

    raw = log.read_text(encoding="utf-8")
    for canary in canaries.values():
        assert canary not in raw
    assert STUB_TOKEN not in raw
    assert SECOND_TOKEN not in raw

    lines = [json.loads(line) for line in raw.splitlines() if line]
    header, frames = lines[0], lines[1:]
    assert header["version"] == FRAME_LOG_VERSION_MULTI_CONNECTION
    assert [row["profile"] for row in header["connections"]] == [ALPHA, BETA]
    assert all(frame["profile"] in {ALPHA, BETA} for frame in frames)
    assert {frame["profile"] for frame in frames} == {ALPHA, BETA}
    # One file, one gapless sequence: replay reads native arrival order and
    # needs no merge rule.
    assert [frame["seq"] for frame in frames] == list(range(1, len(frames) + 1))
    assert any(frame.get("redactions") for frame in frames)


@pytest.mark.asyncio
async def test_one_connection_closing_leaves_the_shared_log_open(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """One gateway going away must not end the recording the others are writing."""
    from talaria.recorder.framelog import FrameRecorder
    from talaria.transport.connection_set import recorded_connections

    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    entries = _entries_for(first, second)
    log = tmp_path / "fleet.jsonl"
    recorder = FrameRecorder(
        log, entries[0].endpoint, connections=recorded_connections(entries)
    )
    fleet = ConnectionSet(
        entries,
        build_source_factory(
            provider_for=credential_provider_factory(credentials), recorder=recorder
        ),
    )

    try:
        await fleet.start()
        beta = fleet.source_for(BETA)
        assert beta is not None
        await beta.close()

        alpha = fleet.source_for(ALPHA)
        assert alpha is not None
        outcome = await alpha.call("session.list", timeout=5.0)
        assert outcome.confirmed, outcome.status
    finally:
        await fleet.close()

    frames = [
        json.loads(line)
        for line in log.read_text(encoding="utf-8").splitlines()[1:]
        if line
    ]
    assert any(
        frame["profile"] == ALPHA and frame.get("frame", {}).get("method") == "session.list"
        for frame in frames
    ), "the surviving connection's traffic stopped being recorded"


# ── the merged stream's own lifecycle ────────────────────────────────────


@pytest.mark.asyncio
async def test_a_start_that_connected_nothing_ends_the_merged_stream(
    tmp_path: Path,
) -> None:
    """A consumer must not park forever on a fleet that will never speak.

    This is what a single-connection ``LiveSource`` already does when its one
    dial fails, and the merged stream has to match it or a launch against a
    gateway that is not running would present as a hang rather than as a
    named failure.
    """
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN})
    fleet = _set_for(
        (
            ConnectionEntry(
                profile=ALPHA, endpoint="ws://127.0.0.1:1/api/ws", credential_profile=ALPHA
            ),
        ),
        credentials,
    )

    try:
        await fleet.start()
        seen: list[TaggedFrame] = []

        async def drain() -> None:
            async for tagged in fleet:
                seen.append(tagged)

        await asyncio.wait_for(drain(), timeout=5.0)
        assert seen == []
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_re_ensuring_one_connection_does_not_end_the_merged_stream(
    two_gateways: tuple[StubGateway, StubGateway], tmp_path: Path
) -> None:
    """The stale-end-marker hazard, exercised on a live fleet.

    A connection that is replaced leaves an end marker behind from the source
    it had before. Acting on that marker would retire a connection that is by
    then live and streaming — so the marker carries the generation it belongs
    to, and a superseded one changes nothing. Asserted with a *second*
    connection running throughout, because that is what keeps the merged
    stream open across the replacement.
    """
    first, second = two_gateways
    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    fleet = _set_for(_entries_for(first, second), credentials)
    seen: list[TaggedFrame] = []

    async def drain() -> None:
        async for tagged in fleet:
            seen.append(tagged)

    consumer: asyncio.Task[None] | None = None
    try:
        await fleet.start()
        consumer = asyncio.create_task(drain())

        beta = fleet.source_for(BETA)
        assert beta is not None
        await beta.close()  # this connection goes away under the set
        await asyncio.sleep(0.05)

        report = await fleet.ensure(BETA)
        assert report.reason == "connected"
        assert len(second.sessions) == 2

        before = len([frame for frame in seen if frame.profile == BETA])
        await second.send(
            {
                "jsonrpc": "2.0",
                "method": "event",
                "params": {"type": "gateway.ready", "payload": {}},
            }
        )
        for _ in range(200):
            if len([frame for frame in seen if frame.profile == BETA]) > before:
                break
            await asyncio.sleep(0.01)
        assert len([frame for frame in seen if frame.profile == BETA]) > before, (
            "the replaced connection stopped feeding the merged stream"
        )
        assert not consumer.done()
        # And the connection that was never touched is still on its own epoch.
        alpha = fleet.source_for(ALPHA)
        assert alpha is not None and alpha.state == "connected"
    finally:
        await fleet.close()
        if consumer is not None:
            consumer.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await consumer


@pytest.mark.asyncio
async def test_the_default_profile_reads_its_own_table_then_the_top_level_pair(
    tmp_path: Path,
) -> None:
    """KTD5's one documented fallback, and it goes only one way.

    ``talaria refresh-credential --profile default`` writes
    ``[profiles.default]``; a file that has never been through that command
    holds the same profile's credential as the top-level ``token``. Both must
    answer, or pairing the default profile by name would strand an operator
    whose file predates the flag. No *other* name gets this — see the sibling
    test that a named profile never falls back.
    """
    path = tmp_path / "credentials"

    _write_credentials(path, token="top-level-value")
    from_top = await LoopbackTokenProvider(
        credentials_path=path, allow_prompt=False, profile=DEFAULT_PROFILE_NAME
    ).acquire()
    assert from_top.value == "top-level-value"
    assert from_top.source == "file"

    _write_credentials(
        path, token="top-level-value", profiles={DEFAULT_PROFILE_NAME: "table-value"}
    )
    from_table = await LoopbackTokenProvider(
        credentials_path=path, allow_prompt=False, profile=DEFAULT_PROFILE_NAME
    ).acquire()
    assert from_table.value == "table-value"
    assert from_table.source == "profile-file"


# ── CR2 remediation: four confirmed defects, each pinned by the case that ──
# ── reproduced it. All four were found by adversarial review of U2, not by ──
# ── these tests, so each one below is written to fail against the original. ──


def test_a_run_scoped_hold_outlives_the_last_view(tmp_path: Path) -> None:
    """A view's lifetime is one connection; a hold's is the whole run (CR2 finding 2)."""
    from talaria.recorder.framelog import FrameRecorder, RecordedConnection, RecorderError

    log = tmp_path / "frames.jsonl"
    recorder = FrameRecorder(
        log,
        endpoint="ws://127.0.0.1:9119/api/ws",
        connections=(
            RecordedConnection(profile=ALPHA, endpoint="ws://127.0.0.1:9120/api/ws"),
            RecordedConnection(profile=BETA, endpoint="ws://127.0.0.1:9121/api/ws"),
        ),
    )
    recorder.hold()

    view = recorder.view(ALPHA)
    view.close()

    # The last view is gone and the log is still writable: without the hold
    # this raises, which is precisely the defect.
    survivor = recorder.view(BETA)
    survivor.record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    survivor.close()

    # And the hold is a claim like any other: dropping it with no views left
    # closes the log, so holding one does not leak an open file handle.
    recorder.release_hold()
    with pytest.raises(RecorderError):
        recorder.view(ALPHA).record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))


@pytest.mark.asyncio
async def test_a_fleet_whose_dials_all_fail_keeps_its_recording_open(
    tmp_path: Path,
) -> None:
    """CR2 finding 2's own reproduction: every view minted and released during start.

    A failed dial still builds a source, so its view is minted and then released
    when that source retires. With every entry unreachable the view count
    crosses zero during ``start`` — and before the fix that closed the log the
    set was about to reconnect into.
    """
    from talaria.recorder.framelog import FrameRecorder, RecordedConnection

    credentials = _write_credentials(
        tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN, BETA: SECOND_TOKEN}
    )
    log = tmp_path / "frames.jsonl"
    dead_alpha, dead_beta = "ws://127.0.0.1:1/api/ws", "ws://127.0.0.1:2/api/ws"
    recorder = FrameRecorder(
        log,
        endpoint=dead_alpha,
        connections=(
            RecordedConnection(profile=ALPHA, endpoint=dead_alpha),
            RecordedConnection(profile=BETA, endpoint=dead_beta),
        ),
    )
    entries = (
        ConnectionEntry(profile=ALPHA, endpoint=dead_alpha, credential_profile=ALPHA),
        ConnectionEntry(profile=BETA, endpoint=dead_beta, credential_profile=BETA),
    )
    fleet = ConnectionSet(
        entries,
        build_source_factory(
            provider_for=credential_provider_factory(credentials), recorder=recorder
        ),
        recorder=recorder,
    )
    try:
        await fleet.start()
        # Still writable after every dial failed and every view was released.
        recorder.view(ALPHA).record("in", json.dumps({"jsonrpc": "2.0", "method": "event"}))
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_a_failed_ensure_wakes_a_consumer_parked_behind_it(
    two_gateways: Any, tmp_path: Path
) -> None:
    """A dead gateway must present as a named failure, never as a hang (CR2 finding 3).

    The choreography is the whole test, and the first attempt at it was vacuous:
    a fleet whose only dial failed has already finished by the time ``ensure``
    runs, so the wake is never needed and removing it changed nothing. The
    hazard needs the ensure to be **mid-flight** — holding the settling guard
    above zero — at the moment the last end marker is drained. Then the
    consumer parks on an empty queue, the settling guard returns to zero when
    the dial fails, the finish condition becomes true, and without a wake there
    is nothing left to notice.

    Staged here by gating the replacement source's ``start`` on an event the
    test controls, which is the only way to hold that window open reliably.
    """
    first, _second = two_gateways
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN})
    entries = (
        ConnectionEntry(profile=ALPHA, endpoint=first.url, credential_profile=ALPHA),
    )

    gate = asyncio.Event()
    built: list[Any] = []
    base = build_source_factory(provider_for=credential_provider_factory(credentials))

    def factory(entry: ConnectionEntry) -> Any:
        source = base(entry)
        source._reconnect_delays = (0.0,)  # noqa: SLF001 - the test wants a fast give-up
        built.append(source)
        if len(built) > 1:
            original = source.start

            async def gated_start() -> Any:
                await gate.wait()
                return await original()

            source.start = gated_start  # type: ignore[method-assign]
        return source

    fleet = ConnectionSet(entries, factory)
    seen: list[TaggedFrame] = []

    async def drain() -> None:
        async for tagged in fleet:
            seen.append(tagged)

    try:
        await fleet.start()
        # The gateway goes away, so the ensure below takes the retire-and-
        # rebuild path rather than reporting ``already_up``.
        await first.stop()
        for _ in range(400):
            if built[0].state != "connected":
                break
            await asyncio.sleep(0.01)

        ensuring = asyncio.create_task(fleet.ensure(ALPHA))
        # Wait until the replacement exists: that happens inside the settling
        # guard, so from here the window is open.
        for _ in range(400):
            if len(built) > 1:
                break
            await asyncio.sleep(0.01)
        assert len(built) > 1, "the ensure never reached the rebuild path"

        # The consumer drains the retired connection's end marker and parks,
        # because the in-flight ensure keeps the finish condition false.
        consumer = asyncio.create_task(drain())
        await asyncio.sleep(0.05)
        assert not consumer.done(), "the consumer should still be waiting here"

        # The dial now fails. Nothing else will ever be enqueued.
        gate.set()
        report = await ensuring

        # Both halves of the contract: the ensure failed *by name*, and the
        # merged stream ended rather than waiting on a connection nobody is
        # feeding. The timeout is the assertion — without the wake this call
        # never returns.
        assert report.reason != "connected", report
        await asyncio.wait_for(consumer, timeout=5.0)
        assert consumer.exception() is None
    finally:
        await fleet.close()


@pytest.mark.asyncio
async def test_the_default_connection_reads_the_default_profile_table(
    tmp_path: Path,
) -> None:
    """``refresh-credential --profile default`` must not write an unread entry (CR2 finding 4).

    The planned default entry carried ``credential_profile=None``, which skips
    the profile level entirely — so ``[profiles.default]`` was invisible to the
    dial while the CLI wrote it happily: a successful-looking pairing followed
    by auth failures against a stale top-level token.
    """
    path = _write_credentials(
        tmp_path / "credentials",
        token="stale-top-level-token",
        profiles={DEFAULT_PROFILE_NAME: "fresh-default-profile-token"},
    )
    plan = plan_connections(default_endpoint="ws://127.0.0.1:9119/api/ws")
    provider = credential_provider_factory(path)(plan[0])

    credential = await provider.acquire()

    assert credential.value == "fresh-default-profile-token"
    assert credential.source == "profile-file"


@pytest.mark.asyncio
async def test_the_default_connection_still_falls_back_to_the_top_level_pair(
    tmp_path: Path,
) -> None:
    """KTD5's one documented fallback: the top-level pair *is* the default's entry."""
    path = _write_credentials(tmp_path / "credentials", token="only-top-level-token")
    plan = plan_connections(default_endpoint="ws://127.0.0.1:9119/api/ws")
    provider = credential_provider_factory(path)(plan[0])

    credential = await provider.acquire()

    assert credential.value == "only-top-level-token"
    assert credential.source == "file"


@pytest.mark.asyncio
async def test_closing_during_an_in_flight_ensure_still_ends_the_stream(
    two_gateways: Any, tmp_path: Path
) -> None:
    """Teardown must terminate the stream even mid-ensure (CR2 round 3).

    The wake sentinel was first written behind a ``not self._closed`` guard,
    which looked like tidiness and was a second hang of the same family:
    ``close`` enqueues its own sentinel, the consumer can drain that one while
    this ensure still holds the settling guard above zero, and then the guard
    suppressed the only wake left. Quitting while a ``/profiles`` dial was
    reaching an unreachable gateway parked the stream forever.
    """
    first, _second = two_gateways
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN})
    entries = (
        ConnectionEntry(profile=ALPHA, endpoint=first.url, credential_profile=ALPHA),
    )

    gate = asyncio.Event()
    built: list[Any] = []
    base = build_source_factory(provider_for=credential_provider_factory(credentials))

    def factory(entry: ConnectionEntry) -> Any:
        source = base(entry)
        source._reconnect_delays = (0.0,)  # noqa: SLF001 - the test wants a fast give-up
        built.append(source)
        if len(built) > 1:
            original = source.start

            async def gated_start() -> Any:
                await gate.wait()
                return await original()

            source.start = gated_start  # type: ignore[method-assign]
        return source

    fleet = ConnectionSet(entries, factory)

    async def drain() -> None:
        async for _tagged in fleet:
            pass

    await fleet.start()
    await first.stop()
    for _ in range(400):
        if built[0].state != "connected":
            break
        await asyncio.sleep(0.01)

    ensuring = asyncio.create_task(fleet.ensure(ALPHA))
    for _ in range(400):
        if len(built) > 1:
            break
        await asyncio.sleep(0.01)
    assert len(built) > 1, "the ensure never reached the rebuild path"

    consumer = asyncio.create_task(drain())
    await asyncio.sleep(0.05)

    # Teardown lands while the ensure is still settling.
    closing = asyncio.create_task(fleet.close())
    await asyncio.sleep(0.05)

    gate.set()
    await asyncio.wait_for(ensuring, timeout=5.0)
    await asyncio.wait_for(closing, timeout=5.0)
    # The assertion is the timeout: with the wake suppressed after close, this
    # consumer never returns.
    await asyncio.wait_for(consumer, timeout=5.0)
    assert consumer.exception() is None


@pytest.mark.asyncio
async def test_a_dial_that_lands_after_close_does_not_leak_its_socket(
    two_gateways: Any, tmp_path: Path
) -> None:
    """A socket acquired after teardown must not outlive it (CR2 rounds 4 and 5).

    The gate goes on the **credential acquire**, which is inside the dial, and
    that placement is the whole point. An earlier version of this test gated
    ``LiveSource.start`` instead — so when the gate opened after close, start's
    own closed-guard returned before any dial ever ran. It pinned the report
    reason and never opened a socket at all, which is precisely the kind of test
    that looks like coverage and is not.

    Gating the acquire puts ``close`` inside the window where the attach is
    genuinely in flight: teardown finds no connection to drop, then the attach
    resolves with a live authenticated socket and nothing left that would ever
    close it. Verified before the fix by pinging that socket after
    ``ConnectionSet.close()`` had returned.
    """
    first, _second = two_gateways
    credentials = _write_credentials(tmp_path / "credentials", profiles={ALPHA: STUB_TOKEN})
    entries = (
        ConnectionEntry(profile=ALPHA, endpoint=first.url, credential_profile=ALPHA),
    )

    gate = asyncio.Event()
    acquires = {"n": 0}
    inner = credential_provider_factory(credentials)

    def provider_for(entry: ConnectionEntry) -> Any:
        provider = inner(entry)
        original = provider.acquire

        async def gated_acquire() -> Any:
            acquires["n"] += 1
            if acquires["n"] > 1:
                await gate.wait()
            return await original()

        provider.acquire = gated_acquire  # type: ignore[method-assign]
        return provider

    built: list[Any] = []
    base = build_source_factory(provider_for=provider_for)

    def factory(entry: ConnectionEntry) -> Any:
        source = base(entry)
        built.append(source)
        return source

    fleet = ConnectionSet(entries, factory)
    await fleet.start()
    # Close the first source so the ensure rebuilds — against a gateway that is
    # still alive, so the replacement dial would succeed if it completed.
    await built[0].close()

    ensuring = asyncio.create_task(fleet.ensure(ALPHA))
    for _ in range(400):
        if acquires["n"] > 1:
            break
        await asyncio.sleep(0.01)
    assert acquires["n"] > 1, "the replacement dial never reached the credential acquire"

    # Teardown lands with the attach genuinely in flight.
    closing = asyncio.create_task(fleet.close())
    await asyncio.sleep(0.05)

    gate.set()
    await asyncio.wait_for(ensuring, timeout=5.0)
    await asyncio.wait_for(closing, timeout=5.0)

    replacement = built[1]
    assert replacement.closed
    # The socket the attach opened is not left running behind the closed set.
    assert getattr(replacement, "_connection", None) is None
