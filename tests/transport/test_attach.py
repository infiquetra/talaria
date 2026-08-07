"""R1, R9, KTD11, KTD13: the credential arrives in the query and nowhere else.

The load-bearing assertions here are the ones about where the credential is
*not*. It is easy to write a test that proves attach works; what R1 and R9 ask
for is proof that the token is absent from argv, from any recorded URL, from any
child environment, and from every object that holds it in memory long enough to
be printed. So most of this file is a canary sweep: one distinctive string is
used as the credential, and each surface is searched for it.
"""

from __future__ import annotations

import asyncio
import dataclasses
import getpass
import inspect
import json
import os
import signal
import sys
from pathlib import Path
from typing import Any, get_args

import pytest

from talaria.recorder.framelog import FrameRecorder
from talaria.status.contract import build_child_env
from talaria.transport import credentials as credentials_module
from talaria.transport.attach import (
    AUTH_CLOSE_CODES,
    AttachFailure,
    AttachOutcome,
    AttachSuccess,
    AttachTarget,
    GatewayConnection,
    attach,
    classify_dial_error,
    strip_credential_query,
    url_carries_credential,
)
from talaria.transport.credentials import (
    GATEWAY_URL_ENV_VAR,
    Credential,
    CredentialError,
    CredentialProvider,
    CredentialSource,
    LoopbackTokenProvider,
    resolve_endpoint,
)
from tests.transport.conftest import STUB_TOKEN, StubGateway

#: One distinctive value, searched for across every surface below.
CANARY = "canary-Ttbdq2Rk-do-not-leak"

#: The variable Hermes's dashboard publishes and Talaria **no longer reads**.
#: KTD8 (2026-08-06) deleted it from the precedence chain and deleted the
#: production constant that named it, so this file holds the literal. It is used
#: two ways below and only two: to prove the chain ignores it, and to prove
#: KTD5's child-environment deny still refuses to forward it. A variable name,
#: not a credential (bandit B105).
RETIRED_TOKEN_ENV_VAR = "HERMES_DASHBOARD_SESSION_TOKEN"  # nosec B105


def endpoint_with_token(value: str, *, host: str = "ws://127.0.0.1:9119/api/ws") -> str:
    """An endpoint carrying ``value`` as a ``token`` query parameter.

    **This helper's meaning inverted on 2026-08-07 and the name was kept.** It
    used to build the string that *answered* the credential chain — a ``token``
    on ``TALARIA_GATEWAY_URL`` was the highest precedence level. That route is
    gone, and an endpoint configured this way is now refused
    (:func:`~talaria.transport.credentials.resolve_endpoint`). Every caller below
    is therefore asserting a refusal or an absence, never a resolution. The name
    still describes exactly what the string *is*, which is what a helper name is
    for; renaming it to ``refused_endpoint`` would describe what one caller
    expects to happen to it.
    """
    return f"{host}?token={value}"


def reachable_strings(value: Any, *, depth: int = 6) -> list[str]:
    """Every string reachable from ``value`` by ordinary attribute traversal.

    The sweep a credential test needs, because "the outcome does not carry the
    token" is a claim about the whole object graph a caller is handed, not about
    the one field the test author thought to check.

    The live :class:`~talaria.transport.attach.GatewayConnection` is a deliberate
    boundary. It is the dialler's own handle, and ``websockets`` keeps the
    request it sent on it — ``connection.request.path`` is
    ``/api/ws?token=…`` and cannot be otherwise, because that is the socket that
    was dialled. Walking into it would assert something no client library can
    satisfy. What is asserted instead is that the credential is not reachable by
    any *other* route, and that the object's own repr does not print it.
    """
    found: list[str] = []
    seen: set[int] = set()

    def walk(item: Any, budget: int) -> None:
        if budget < 0 or id(item) in seen:
            return
        seen.add(id(item))
        if isinstance(item, str):
            found.append(item)
            return
        if isinstance(item, GatewayConnection):
            return
        if isinstance(item, bytes):
            found.append(item.decode("utf-8", "replace"))
            return
        if isinstance(item, dict):
            for key, child in item.items():
                walk(key, budget - 1)
                walk(child, budget - 1)
            return
        if isinstance(item, list | tuple | set | frozenset):
            for child in item:
                walk(child, budget - 1)
            return
        if dataclasses.is_dataclass(item) and not isinstance(item, type):
            for field in dataclasses.fields(item):
                walk(getattr(item, field.name, None), budget - 1)
            return
        for child in vars(item).values() if hasattr(item, "__dict__") else ():
            walk(child, budget - 1)

    walk(value, depth)
    return found


def assert_outcome_withholds(outcome: AttachOutcome, secret: str) -> None:
    """The whole sweep, in the one place every credential test can reuse."""
    assert secret not in repr(outcome)
    assert secret not in str(outcome)
    assert secret not in f"{outcome!r}"
    leaked = [text for text in reachable_strings(outcome) if secret in text]
    assert leaked == [], f"the attach outcome carries the credential: {leaked}"


# ── the dial itself ──────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_the_credential_arrives_as_the_token_query_parameter(
    gateway: StubGateway,
) -> None:
    """KTD13's loopback pin: ``ws.query_params['token']``, never a header."""
    target = AttachTarget.from_url(gateway.url)
    outcome = await attach(target, Credential("token", STUB_TOKEN, "file"))

    assert isinstance(outcome, AttachSuccess)
    await outcome.connection.close()

    assert gateway.queries == [{"token": STUB_TOKEN}]
    assert gateway.paths[0].startswith("/api/ws?")
    assert "token=" in gateway.paths[0]
    # The success outcome carries the redacted endpoint and not the URL that was
    # dialled, so no caller downstream can leak what it was never handed.
    assert STUB_TOKEN not in outcome.safe_url
    assert outcome.safe_url == target.safe_url


@pytest.mark.asyncio
async def test_the_credential_arrives_in_the_query_and_in_no_header(
    gateway: StubGateway,
) -> None:
    """KTD13's "and nowhere else" half, asserted against what the server saw.

    Recording only ``request.path`` made this claim untestable, and untestably
    false: adding an ``X-Talaria-Token`` header to every dial passed the whole
    suite. The stub now keeps the entire handshake, so the sweep below covers
    every header name and value — which is where a duplicated credential would
    be: a bearer ``Authorization``, a ``Cookie``, or a ``Sec-WebSocket-Protocol``
    smuggling the token as a subprotocol name.
    """
    gateway.token = CANARY
    target = AttachTarget.from_url(gateway.url)
    outcome = await attach(target, Credential("token", CANARY, "file"))

    assert isinstance(outcome, AttachSuccess)
    await outcome.connection.close()

    handshake = gateway.handshakes[0]
    assert handshake.query == {"token": CANARY}, "the credential did not ride the query"
    elsewhere = [part for part in handshake.everything_but_the_query() if CANARY in part]
    assert elsewhere == [], f"the credential was duplicated outside the query: {elsewhere}"
    assert any(name.lower() == "host" for name, _ in handshake.headers), (
        "the stub recorded no headers at all, so this proves nothing"
    )


@pytest.mark.asyncio
async def test_a_wrong_credential_yields_the_named_authentication_failed_state(
    gateway: StubGateway,
) -> None:
    """F1/R35: the failure is named, not a generic connection error."""
    target = AttachTarget.from_url(gateway.url)
    outcome = await attach(target, Credential("token", "the-wrong-token", "file"))

    assert isinstance(outcome, AttachFailure)
    assert outcome.kind == "auth_failed"
    assert gateway.rejections == 1
    assert gateway.sessions == []
    # The detail names the endpoint, and cannot name the credential: the
    # endpoint the failure is built from was stripped of one before the dial.
    assert "the-wrong-token" not in outcome.detail
    assert "127.0.0.1" in outcome.detail


@pytest.mark.asyncio
async def test_an_unreachable_gateway_is_connect_failed_not_auth_failed() -> None:
    """R35 keeps the two apart; conflating them sends the operator to the wrong fix."""
    # Port 1 on loopback: reserved, and nothing listens there.
    target = AttachTarget.from_url("ws://127.0.0.1:1/api/ws")
    outcome = await attach(target, Credential("token", STUB_TOKEN, "file"))

    assert isinstance(outcome, AttachFailure)
    assert outcome.kind == "connect_failed"


def test_every_authentication_close_code_classifies_as_auth_failed() -> None:
    """The 4401 the gateway actually sends (``web_server.py:15615-15617``)."""

    class _Frame:
        def __init__(self, code: int) -> None:
            self.code = code

    class _Closed(Exception):
        def __init__(self, code: int) -> None:
            super().__init__(f"closed {code}")
            self.rcvd = _Frame(code)
            self.sent = None

    for code in AUTH_CLOSE_CODES:
        assert classify_dial_error(_Closed(code)) == "auth_failed"
    # 4403 means "embedded chat disabled" or "request not allowed" at this pin,
    # not a rejected credential. Telling the operator to check their token would
    # send them to the wrong place.
    assert classify_dial_error(_Closed(4403)) == "connect_failed"
    assert classify_dial_error(OSError("connection refused")) == "connect_failed"


def test_an_http_handshake_rejection_also_classifies_as_auth_failed() -> None:
    """The other shape a pre-accept ``close(4401)`` can reach the client as.

    Which of the two the ASGI server produces is its choice, not Hermes's, so
    both are classified — see the module docstring in ``transport/attach.py``.
    """

    class _Response:
        status_code = 403

    class _InvalidStatus(Exception):
        response = _Response()

    class _InvalidStatusCode(Exception):
        status_code = 401

    assert classify_dial_error(_InvalidStatus()) == "auth_failed"
    assert classify_dial_error(_InvalidStatusCode()) == "auth_failed"


# ── the failure detail is operator-facing, so it is credential-facing ────


#: Two endpoint typos that make ``websockets`` raise ``InvalidURI`` — whose
#: ``__str__`` is the whole dialled URI, credential and all. The first is an
#: ``http``/``ws`` scheme mix-up, which is the single most likely thing to be in
#: ``TALARIA_GATEWAY_URL``; the second omits the host.
LEAKY_ENDPOINTS = ["http://127.0.0.1:9119/api/ws", "ws:///api/ws"]


@pytest.mark.parametrize("endpoint", LEAKY_ENDPOINTS)
@pytest.mark.asyncio
async def test_a_dial_error_never_puts_the_credential_in_the_failure_detail(
    endpoint: str,
) -> None:
    """The detail is shown to the operator, so it is a credential surface.

    ``websockets`` raises ``InvalidURI(uri, msg)`` and its message is the whole
    dialled URI — which is the *credentialed* one, because that is the only
    string the dialler is ever handed. Interpolating ``str(exc)`` therefore put
    the live token into ``AttachFailure.detail``, and from there into
    ``LiveSource.last_failure`` and the composer notice.
    """
    target = AttachTarget.from_url(endpoint)
    outcome = await attach(target, Credential("token", CANARY, "file"))

    assert isinstance(outcome, AttachFailure)
    assert CANARY not in outcome.detail
    assert_outcome_withholds(outcome, CANARY)
    # Not merely absent — withheld, and still saying what went wrong. The marker
    # is percent-encoded when it lands inside a query value, which is the same
    # on-disk convention the frame log's own ``endpoint`` header uses.
    assert "redacted" in outcome.detail
    assert "valid URI" in outcome.detail


@pytest.mark.asyncio
async def test_a_dial_error_still_says_what_went_wrong() -> None:
    """Over-redaction is a different failure: the detail has to stay useful."""
    target = AttachTarget.from_url("ws://127.0.0.1:1/api/ws")
    outcome = await attach(target, Credential("token", CANARY, "file"))

    assert isinstance(outcome, AttachFailure)
    assert outcome.kind == "connect_failed"
    assert "127.0.0.1:1" in outcome.detail
    assert outcome.detail != "[redacted]"


@pytest.mark.asyncio
async def test_a_cancelled_dial_unwinds_rather_than_becoming_a_failure() -> None:
    """A cancelled dial is Talaria being torn down, not a gateway being absent.

    Swallowing it produced ``AttachFailure(kind='connect_failed',
    detail='…: CancelledError')`` — a false statement about the gateway — and
    left whatever requested the cancellation waiting on a coroutine that had
    decided to return normally instead.
    """

    async def _never_finishes(url: str) -> GatewayConnection:
        await asyncio.Event().wait()
        raise AssertionError("unreachable")  # pragma: no cover

    target = AttachTarget.from_url("ws://127.0.0.1:9119/api/ws")
    task = asyncio.ensure_future(
        attach(target, Credential("token", CANARY, "file"), dialer=_never_finishes)
    )
    await asyncio.sleep(0.01)
    task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task


# ── URL hygiene (R1, R9, PC10) ───────────────────────────────────────────


@pytest.mark.parametrize("key", ["token", "ticket", "internal"])
def test_an_attach_target_is_credential_free_by_construction(key: str) -> None:
    """All three credential forms the upgrade path accepts are stripped.

    ``ticket`` and ``internal`` are not forms v0.1 produces, but an operator can
    paste an endpoint that carries one out of a browser session, and neither is
    matched by ``redactUrl``'s inherited pattern set (KTD11).
    """
    target = AttachTarget.from_url(f"ws://127.0.0.1:9119/api/ws?{key}={CANARY}&x=1")
    assert CANARY not in target.url
    assert CANARY not in target.safe_url
    assert "x=1" in target.url


def test_stripping_leaves_a_credential_free_url_byte_identical() -> None:
    """Normalizing an untouched URL would change bytes KTD6 compares."""
    url = "ws://127.0.0.1:9119/api/ws?mode=chat&x=%2Ffoo"
    assert strip_credential_query(url) == url
    assert strip_credential_query("ws://127.0.0.1:9119/api/ws") == "ws://127.0.0.1:9119/api/ws"


def test_the_dial_url_is_the_only_place_the_credential_appears() -> None:
    target = AttachTarget.from_url("ws://127.0.0.1:9119/api/ws")
    dial = target.dial_url(Credential("token", CANARY, "file"))

    assert f"token={CANARY}" in dial
    assert CANARY not in target.url
    assert CANARY not in target.safe_url
    assert CANARY not in repr(target)


def test_a_credential_on_the_endpoint_is_replaced_not_duplicated() -> None:
    """Otherwise a stale pasted token would ride alongside the live one."""
    target = AttachTarget.from_url(f"ws://127.0.0.1:9119/api/ws?token={CANARY}")
    dial = target.dial_url(Credential("token", "fresh", "file"))
    assert dial.count("token=") == 1
    assert CANARY not in dial


def test_the_recorded_endpoint_header_withholds_a_credential(tmp_path: Path) -> None:
    """R9: no URL that reaches a record carries one.

    Recorded through the real :class:`FrameRecorder`, not through
    ``redact_url`` directly — the requirement is about what lands on disk, and
    a test of the redactor alone would pass even if the recorder stopped
    calling it.
    """
    log = tmp_path / "frames.jsonl"
    recorder = FrameRecorder(log, f"ws://127.0.0.1:9119/api/ws?token={CANARY}")
    recorder.close()

    text = log.read_text(encoding="utf-8")
    assert CANARY not in text
    header = json.loads(text.splitlines()[0])
    assert header["kind"] == "header"
    assert "redacted" in header["endpoint"]


def test_the_status_child_environment_carries_no_credential() -> None:
    """KTD5's canary: the whole query is dropped, not pattern-filtered.

    Deliberately still exercised with :data:`RETIRED_TOKEN_ENV_VAR` in the parent
    environment. KTD8 removed that variable from Talaria's credential chain, not
    from operators' shells, and ``talaria/status/contract.py``'s deny is what
    stops an inherited one from being handed to a status child. Dropping it from
    this fixture because "Talaria does not read it any more" would retire the
    coverage of the boundary that still has to hold.
    """
    parent = {
        "PATH": "/usr/bin",
        "HOME": "/home/op",
        GATEWAY_URL_ENV_VAR: endpoint_with_token(CANARY),
        RETIRED_TOKEN_ENV_VAR: CANARY,
        "TALARIA_PROFILE": "default",
    }
    child = build_child_env(parent_env=parent, allowlist=(RETIRED_TOKEN_ENV_VAR,))

    assert CANARY not in "\n".join(f"{k}={v}" for k, v in child.items())
    assert child[GATEWAY_URL_ENV_VAR] == "ws://127.0.0.1:9119/api/ws"
    # Even an operator who explicitly allowlists the token variable does not get
    # it forwarded: the credential-shaped-name deny is non-overridable (KTD5).
    assert RETIRED_TOKEN_ENV_VAR not in child


@pytest.mark.asyncio
async def test_the_attach_outcome_carries_no_credential_anywhere(
    gateway: StubGateway,
    tmp_path: Path,
) -> None:
    """R1, restated as something that can fail.

    This test used to read the *pytest process's own* ``sys.argv`` and assert the
    credential was not in it. Nothing under test writes ``sys.argv``, so the
    assertion held no matter what the transport did: adding a ``dialed_url``
    field carrying the full credentialed URL to :class:`AttachSuccess` passed the
    entire suite. R1's real content is that no object handed back to a caller
    carries the value, so that is what is swept — every string reachable from the
    outcome, plus its repr, which is the surface a traceback or a ``%r`` log line
    would print.

    ``sys.argv`` is still checked, because "no child process was spawned to hold
    it" is a genuine part of the claim — it is just not the load-bearing part.

    The credential comes from a ``0600`` file rather than from an exported
    endpoint, because that is the only non-interactive route left after
    2026-08-07 — and it is the stronger setup anyway: the value under sweep was
    never in the environment to begin with.
    """
    path = tmp_path / "credentials"
    path.write_text(f'token = "{CANARY}"\n', encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(credentials_path=path)
    target = AttachTarget.from_url(gateway.url)
    gateway.token = CANARY

    outcome = await attach(target, await provider.acquire())
    assert isinstance(outcome, AttachSuccess)

    assert_outcome_withholds(outcome, CANARY)
    await outcome.connection.close()

    assert CANARY not in " ".join(sys.argv)


@pytest.mark.asyncio
async def test_the_sweep_would_catch_a_credential_added_to_the_outcome() -> None:
    """Guard the guard: a sweep that cannot fail proves nothing.

    The exact defect the previous ``sys.argv`` test missed, reconstructed on a
    stand-in outcome — a field carrying the dialled URL — so the sweep is known
    to be looking in the right place.
    """

    @dataclasses.dataclass(frozen=True)
    class LeakyOutcome:
        safe_url: str
        dialed_url: str

    leaky = LeakyOutcome(
        safe_url="ws://127.0.0.1:9119/api/ws",
        dialed_url=f"ws://127.0.0.1:9119/api/ws?token={CANARY}",
    )
    with pytest.raises(AssertionError):
        assert_outcome_withholds(leaky, CANARY)  # type: ignore[arg-type]


def test_the_credential_object_withholds_its_value_from_every_repr() -> None:
    """A generated dataclass repr would put the token in every traceback."""
    credential = Credential("token", CANARY, "prompt")
    assert CANARY not in repr(credential)
    assert CANARY not in str(credential)
    assert CANARY not in f"{credential}"
    assert CANARY not in "{!r}".format(credential)  # noqa: UP032 - exercising %r/format
    assert credential.value == CANARY


# ── the acquisition chain (KTD11) ────────────────────────────────────────


@pytest.mark.asyncio
async def test_no_environment_variable_supplies_a_credential(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """Both retired levels at once, measured against the **real** environment.

    The chain had two environment-borne levels and now has none:
    ``HERMES_DASHBOARD_SESSION_TOKEN`` went on 2026-08-06 (KTD8), and a ``token``
    on ``TALARIA_GATEWAY_URL`` went on 2026-08-07. This sets both to a canary and
    asserts the chain resolves nothing at all.

    ``monkeypatch.setenv`` rather than an injected mapping, and that is the whole
    point: :class:`LoopbackTokenProvider` no longer *takes* an environment, so
    the real one is the only one there is to test against. A test that could
    hand the provider a mapping would be testing a seam the shipped code does
    not have.

    What this does **not** assert, because it is not true: that either variable
    has become invisible. Both are right there in the process environment. They
    are simply unreachable from here.
    """
    monkeypatch.setenv(RETIRED_TOKEN_ENV_VAR, CANARY)
    monkeypatch.setenv(GATEWAY_URL_ENV_VAR, endpoint_with_token(CANARY))

    provider = LoopbackTokenProvider(
        credentials_path=tmp_path / "absent",
        allow_prompt=False,
    )
    with pytest.raises(CredentialError) as caught:
        await provider.acquire()

    message = str(caught.value)
    assert CANARY not in message
    for retired in (RETIRED_TOKEN_ENV_VAR, GATEWAY_URL_ENV_VAR):
        assert retired not in message, (
            f"the refusal advertises {retired}, a route that no longer resolves anything"
        )


def test_the_provider_has_no_environment_injection_point() -> None:
    """The structural half of the removal, and the reason it is structural.

    Deleting the *level* while keeping an ``environ`` parameter would leave the
    provider able to see a variable it must never read, and would leave a test
    able to hand one in and observe a precedence chain the shipped code cannot
    walk. Both retired levels were reachable through this parameter, so it went
    with the second one.

    Asserted twice on purpose. The signature check states the claim in the form
    a reader can act on; the ``TypeError`` check proves the parameter is really
    gone rather than merely undocumented, which is what a reintroduction would
    look like from the outside.
    """
    parameters = inspect.signature(LoopbackTokenProvider).parameters
    assert "environ" not in parameters, (
        "the credential provider can reach an environment again; no level should "
        "be able to read one (R1)"
    )
    with pytest.raises(TypeError):
        LoopbackTokenProvider(environ={})  # type: ignore[call-arg]


def test_no_credential_source_names_an_environment_variable() -> None:
    """A label nothing can produce is a chain a test can still claim to observe.

    ``CredentialSource`` has lost two members, each alongside the level that
    produced it: ``"environment"`` with ``HERMES_DASHBOARD_SESSION_TOKEN``, and
    ``"endpoint-url"`` with the ``token``-on-``TALARIA_GATEWAY_URL`` route. This
    pins the surviving set, so adding an environment-borne source back means
    editing this test on purpose — the cheapest place for that decision to
    become visible in review.
    """
    assert set(get_args(CredentialSource)) == {"file", "prompt", "prompt-cached"}


@pytest.mark.asyncio
async def test_the_file_wins_over_the_prompt(credentials_file: Path) -> None:
    def _never(label: str) -> str:  # pragma: no cover - asserted not to run
        raise AssertionError("the prompt was reached with a readable credential file")

    provider = LoopbackTokenProvider(credentials_path=credentials_file, prompt=_never)
    credential = await provider.acquire()
    assert (credential.value, credential.source) == (STUB_TOKEN, "file")


@pytest.mark.asyncio
async def test_the_prompt_is_the_last_resort(tmp_path: Path) -> None:
    provider = LoopbackTokenProvider(
        credentials_path=tmp_path / "absent",
        prompt=lambda label: "typed-by-the-operator",
    )
    credential = await provider.acquire()
    assert (credential.value, credential.source) == ("typed-by-the-operator", "prompt")


@pytest.mark.parametrize("mode", [0o644, 0o640, 0o604, 0o666])
@pytest.mark.asyncio
async def test_a_credential_file_looser_than_0600_is_refused(tmp_path: Path, mode: int) -> None:
    """The check runs *before* the read: a file the machine can read has leaked."""
    path = tmp_path / "credentials"
    path.write_text('token = "should-not-be-read"\n', encoding="utf-8")
    path.chmod(mode)

    provider = LoopbackTokenProvider(
        credentials_path=path,
    )
    with pytest.raises(CredentialError) as caught:
        await provider.acquire()
    assert f"{mode:04o}" in str(caught.value)
    assert "should-not-be-read" not in str(caught.value)


@pytest.mark.asyncio
async def test_a_credential_file_at_0600_is_accepted(tmp_path: Path) -> None:
    path = tmp_path / "credentials"
    path.write_text('token = "accepted"\n', encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(
        credentials_path=path,
    )
    assert (await provider.acquire()).value == "accepted"


@pytest.mark.asyncio
async def test_a_malformed_credential_file_is_named_not_guessed(tmp_path: Path) -> None:
    path = tmp_path / "credentials"
    path.write_text("this is not toml at all\n", encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(credentials_path=path, allow_prompt=False)
    with pytest.raises(CredentialError) as caught:
        await provider.acquire()
    assert 'token = "' in str(caught.value)


@pytest.mark.asyncio
async def test_a_trailing_newline_in_the_credential_file_is_stripped(
    tmp_path: Path,
) -> None:
    """``echo "$TOKEN" > credentials`` is how this file gets written."""
    path = tmp_path / "credentials"
    path.write_text('token = "  padded  "\n', encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(
        credentials_path=path,
    )
    assert (await provider.acquire()).value == "padded"


@pytest.mark.asyncio
async def test_with_no_source_and_no_prompt_the_failure_says_what_to_do(
    tmp_path: Path,
) -> None:
    """And what it says must be a route that still exists.

    A refusal is the one place an operator reads a route list at the moment they
    need it, so this asserts both halves: every surviving route is named, and
    **neither** retired route is. Sending someone who cannot authenticate to a
    route that silently resolves nothing is the worst possible moment to be wrong
    about what works.
    """
    provider = LoopbackTokenProvider(credentials_path=tmp_path / "absent", allow_prompt=False)
    with pytest.raises(CredentialError) as caught:
        await provider.acquire()
    message = str(caught.value)
    assert str(tmp_path / "absent") in message
    assert "talaria refresh-credential" in message
    for retired in (GATEWAY_URL_ENV_VAR, RETIRED_TOKEN_ENV_VAR):
        assert retired not in message, f"the refusal advertises {retired}, which resolves nothing"


@pytest.mark.asyncio
async def test_a_prompt_with_no_terminal_is_a_named_credential_failure(
    tmp_path: Path,
) -> None:
    """The launcher path found this one (U10). ``getpass`` falls back to reading
    stdin when there is no terminal, and a closed stdin makes that an
    ``EOFError`` — which is not a :class:`CredentialError` and used to escape
    this module entirely. Three layers up it surfaced as "the frame stream
    failed", which tells an operator running Talaria under a supervisor nothing
    about the credential they have not supplied.

    The message has to name a way out, so every surviving route is asserted —
    and that neither retired route is among them."""

    def _no_terminal(label: str) -> str:
        raise EOFError("EOF when reading a line")

    provider = LoopbackTokenProvider(credentials_path=tmp_path / "absent", prompt=_no_terminal)
    with pytest.raises(CredentialError) as caught:
        await provider.acquire()
    message = str(caught.value)
    assert "talaria refresh-credential" in message
    assert str(tmp_path / "absent") in message
    for retired in (GATEWAY_URL_ENV_VAR, RETIRED_TOKEN_ENV_VAR):
        assert retired not in message, f"the refusal advertises {retired}, which resolves nothing"


@pytest.mark.asyncio
async def test_a_prompt_that_answers_is_still_accepted(tmp_path: Path) -> None:
    """Pairs the test above: turning a broken prompt into a named failure must
    not turn a working one into one."""
    provider = LoopbackTokenProvider(
        credentials_path=tmp_path / "absent", prompt=lambda label: "typed"
    )
    credential = await provider.acquire()
    assert (credential.value, credential.source) == ("typed", "prompt")


# ── per-dial acquisition and prompt caching (KTD11) ──────────────────────


class CountingProvider:
    """A provider double that counts dials and can rotate its answer."""

    def __init__(self, values: list[str]) -> None:
        self.values = values
        self.calls = 0

    async def acquire(self) -> Credential:
        value = self.values[min(self.calls, len(self.values) - 1)]
        self.calls += 1
        return Credential("token", value, "file")


def test_the_provider_double_satisfies_the_protocol() -> None:
    """Guard the guard: a double that is not a provider proves nothing."""
    assert isinstance(CountingProvider(["x"]), CredentialProvider)
    assert isinstance(LoopbackTokenProvider(), CredentialProvider)


@pytest.mark.asyncio
async def test_a_rotated_credential_is_picked_up_without_a_restart(
    tmp_path: Path,
) -> None:
    """KTD11's rotation guarantee, re-expressed against the credential file.

    **This test used to rotate ``HERMES_DASHBOARD_SESSION_TOKEN``.** KTD8
    removed that level, and deleting the test with it would have taken a real
    guarantee along — the one that says a Hermes dashboard restart does not
    require a Talaria restart. So the same claim is made against the route that
    survives: ``talaria refresh-credential`` rewrites
    ``<config_dir>/credentials`` and the next dial reads it, with no process
    restart and no re-prompt.

    The dashboard-restart shape is reproduced rather than approximated: the file
    is rewritten wholesale with a new value at the same ``0600`` mode, exactly
    as ``talaria/transport/refresh.py`` writes it. ``acquisitions`` is asserted
    because KTD11's per-dial rule is otherwise invisible — a provider that
    resolved once and cached would return "first" twice and still look right if
    only the first value were checked.
    """
    path = tmp_path / "credentials"
    path.write_text('token = "first"\n', encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(
        credentials_path=path,
    )

    assert (await provider.acquire()).value == "first"

    path.write_text('token = "rotated"\n', encoding="utf-8")
    path.chmod(0o600)

    rotated = await provider.acquire()
    assert (rotated.value, rotated.source) == ("rotated", "file")
    assert provider.acquisitions == 2


# ``test_a_rotated_token_on_the_endpoint_url_is_picked_up_too`` stood here until
# 2026-08-07. It paired with the file test above to show KTD11's per-dial rule
# covered both rotating levels; there is now one rotating level, so the pair has
# nothing to pair with. Recorded rather than deleted quietly: a rotation test
# that disappears in the same commit as a credential route is exactly the shape
# of a guarantee being dropped by accident, and this one was dropped on purpose
# because the thing it guarded no longer exists.


@pytest.mark.asyncio
async def test_the_file_is_re_read_on_every_acquisition(tmp_path: Path) -> None:
    path = tmp_path / "credentials"
    path.write_text('token = "first"\n', encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(
        credentials_path=path,
    )

    assert (await provider.acquire()).value == "first"
    path.write_text('token = "rotated"\n', encoding="utf-8")
    path.chmod(0o600)
    assert (await provider.acquire()).value == "rotated"


@pytest.mark.asyncio
async def test_a_prompted_credential_is_cached_and_never_re_prompted(
    tmp_path: Path,
) -> None:
    """A reconnect that waits for a human is a reconnect that does not happen."""
    prompts: list[str] = []

    def _prompt(label: str) -> str:
        prompts.append(label)
        return "typed-once"

    provider = LoopbackTokenProvider(credentials_path=tmp_path / "absent", prompt=_prompt)

    first = await provider.acquire()
    second = await provider.acquire()
    third = await provider.acquire()

    assert len(prompts) == 1, "the operator was asked again mid-reconnect"
    assert first.source == "prompt"
    assert second.source == third.source == "prompt-cached"
    assert second.value == third.value == "typed-once"
    assert provider.acquisitions == 3


@pytest.mark.asyncio
async def test_a_credential_file_written_later_outranks_a_cached_prompt(
    tmp_path: Path,
) -> None:
    """Otherwise the cache would pin a stale token past a rotation.

    Re-expressed against the file (KTD8): the operator who typed a token at
    launch and then ran ``talaria refresh-credential`` mid-session must get the
    fresh value on the next dial, not the one they typed. Previously asserted by
    rotating ``HERMES_DASHBOARD_SESSION_TOKEN`` into the environment, which is
    no longer a route — the guarantee is the same one, and it still holds.
    """
    path = tmp_path / "credentials"
    provider = LoopbackTokenProvider(
        credentials_path=path,
        prompt=lambda label: "typed",
    )
    assert (await provider.acquire()).value == "typed"

    path.write_text('token = "rotated-into-the-file"\n', encoding="utf-8")
    path.chmod(0o600)
    credential = await provider.acquire()
    assert (credential.value, credential.source) == (
        "rotated-into-the-file",
        "file",
    )


# ── priming: the prompt happens before the interface, or not at all ──────


@pytest.mark.asyncio
async def test_priming_resolves_without_asking_when_a_file_answers(
    tmp_path: Path,
) -> None:
    """Priming is not "always prompt"; it is "prompt now if at all"."""
    path = tmp_path / "credentials"
    path.write_text('token = "from-the-file"\n', encoding="utf-8")
    path.chmod(0o600)

    asked: list[str] = []

    def _prompt(label: str) -> str:
        asked.append(label)
        return "typed"

    provider = LoopbackTokenProvider(credentials_path=path, prompt=_prompt)

    credential = await provider.prime()

    assert (credential.source, credential.value) == ("file", "from-the-file")
    assert asked == [], "the operator was asked for a credential they had already written"


@pytest.mark.asyncio
async def test_priming_seals_the_prompt_so_no_later_dial_can_ask(tmp_path: Path) -> None:
    """The regression this whole mechanism exists for.

    A dial reaching :func:`getpass.getpass` under a running Textual app writes a
    prompt nowhere visible and blocks on a read that fights the UI for stdin.
    The operator sees "connecting to gateway" forever. After priming, that path
    must be unreachable — not unlikely, unreachable — so the same situation
    surfaces as a named failure instead.
    """
    path = tmp_path / "credentials"
    path.write_text('token = "from-the-file"\n', encoding="utf-8")
    path.chmod(0o600)

    asked: list[str] = []

    def _prompt(label: str) -> str:
        asked.append(label)
        return "typed"

    provider = LoopbackTokenProvider(credentials_path=path, prompt=_prompt)
    assert (await provider.prime()).source == "file"

    # The credential goes away mid-session, which is what a dashboard restart
    # does: the token is minted per server start and dies with the process.
    path.unlink()

    with pytest.raises(CredentialError):
        await provider.acquire()
    assert asked == [], "a dial reached the interactive prompt"


@pytest.mark.asyncio
async def test_priming_seals_the_prompt_even_when_priming_itself_failed(
    tmp_path: Path,
) -> None:
    """Otherwise the one launch with no terminal leaves the prompt armed."""

    asked: list[str] = []

    def _no_terminal(label: str) -> str:
        asked.append(label)
        raise EOFError("not a terminal")

    provider = LoopbackTokenProvider(credentials_path=tmp_path / "absent", prompt=_no_terminal)

    with pytest.raises(CredentialError):
        await provider.prime()
    assert len(asked) == 1

    with pytest.raises(CredentialError):
        await provider.acquire()
    assert len(asked) == 1, "the prompt was still armed after a failed prime"


@pytest.mark.asyncio
async def test_a_primed_answer_serves_every_later_dial(tmp_path: Path) -> None:
    """The operator types once, at launch, and never again."""
    asked: list[str] = []

    def _prompt(label: str) -> str:
        asked.append(label)
        return "typed-at-launch"

    provider = LoopbackTokenProvider(credentials_path=tmp_path / "absent", prompt=_prompt)

    primed = await provider.prime()
    first = await provider.acquire()
    second = await provider.acquire()

    assert len(asked) == 1
    assert primed.source == "prompt"
    assert first.source == second.source == "prompt-cached"
    assert first.value == second.value == "typed-at-launch"


@pytest.mark.asyncio
async def test_priming_is_not_counted_as_a_dial(tmp_path: Path) -> None:
    """``acquisitions`` is KTD11's per-dial evidence, and priming is not a dial.

    A counter that is reliably wrong by one is worse than no counter: the whole
    point of the number is that "called once per dial" is otherwise invisible.
    """
    path = tmp_path / "credentials"
    path.write_text('token = "from-the-file"\n', encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(
        credentials_path=path,
    )

    await provider.prime()
    assert provider.acquisitions == 0

    await provider.acquire()
    await provider.acquire()
    assert provider.acquisitions == 2


@pytest.mark.asyncio
async def test_priming_still_re_reads_the_file_on_every_dial(
    tmp_path: Path,
) -> None:
    """Sealing the prompt must not freeze level 1 into a cached value.

    This test has been narrowed twice as levels left the chain — first when
    ``HERMES_DASHBOARD_SESSION_TOKEN`` went (KTD8), then when the
    ``token``-on-``TALARIA_GATEWAY_URL`` route went on 2026-08-07. One rotating
    level remains, and it is the one ``talaria refresh-credential`` rewrites, so
    this is the whole of what "a rotated credential survives priming" now means.
    """
    path = tmp_path / "credentials"
    path.write_text('token = "first"\n', encoding="utf-8")
    path.chmod(0o600)
    provider = LoopbackTokenProvider(credentials_path=path, allow_prompt=False)

    assert (await provider.prime()).value == "first"

    path.write_text('token = "rotated-after-priming"\n', encoding="utf-8")
    path.chmod(0o600)
    credential = await provider.acquire()
    assert (credential.value, credential.source) == ("rotated-after-priming", "file")


# ── the interactive prompt does not echo (R9) ────────────────────────────


@pytest.mark.asyncio
async def test_a_prompt_with_no_terminal_refuses_rather_than_echoing(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """:func:`getpass.getpass` falls back to an echoing read; that is refused.

    Without the refusal an unattended launch — a supervisor, a CI job, a shell
    with stdin redirected — reads the token through :func:`input` and leaves it
    in that process's scrollback and logs, which is exactly where R9 says a
    credential must never appear. The standard library announces this with a
    warning and proceeds; announcing it is not the same as preventing it.

    What is asserted is that :func:`getpass.getpass` is never *reached*, not
    merely that the call fails. Asserting only the failure proved nothing: under
    pytest stdin is captured, so the fallback hits ``EOFError`` and raises
    ``CredentialError`` whether the guard is there or not — a test that passed
    identically with the guard deleted.
    """
    reached: list[str] = []

    def _echoing_fallback(label: str) -> str:
        reached.append(label)
        return "read-with-echo-on"

    monkeypatch.setattr(getpass, "getpass", _echoing_fallback)
    monkeypatch.setattr(credentials_module, "_has_controlling_terminal", lambda: False)
    provider = LoopbackTokenProvider(
        credentials_path=tmp_path / "absent",
    )

    with pytest.raises(CredentialError) as excinfo:
        await provider.acquire()

    assert reached == [], "the echoing fallback was reached"
    message = str(excinfo.value)
    assert "terminal" in message
    assert "talaria refresh-credential" in message, (
        "a refusal that names no way forward is a dead end"
    )
    for retired in (GATEWAY_URL_ENV_VAR, RETIRED_TOKEN_ENV_VAR):
        assert retired not in message, f"the refusal advertises {retired}, which resolves nothing"


PTY_PROBE = """
import asyncio, sys
sys.path.insert(0, {repo!r})
from talaria.transport.credentials import LoopbackTokenProvider

provider = LoopbackTokenProvider()
credential = asyncio.run(provider.acquire())
print("LENGTH", len(credential.value), flush=True)
"""


def _read_until(master: int, marker: bytes, seen: bytearray, *, budget: float) -> bool:
    """Drain the master side until ``marker`` appears, or the budget runs out."""
    import select

    while marker not in bytes(seen) and budget > 0:
        ready, _, _ = select.select([master], [], [], 0.25)
        budget -= 0.25
        if not ready:
            continue
        try:
            chunk = os.read(master, 4096)
        except OSError:  # the child exited and closed the slave
            break
        if not chunk:
            break
        seen += chunk
    return marker in bytes(seen)


@pytest.mark.skipif(sys.platform == "win32", reason="pty is POSIX-only")
def test_the_interactive_prompt_does_not_echo() -> None:
    """R9, proved against a real terminal rather than by naming a function.

    A pseudo-terminal echoes typed characters by default — that is the line
    discipline, not the program. :func:`getpass.getpass` clears ``ECHO`` through
    ``termios`` for the duration of the read, so if the credential prompt used
    :func:`input` instead, the typed token would appear in the master side's
    output. Asserting only that the module *references* ``getpass`` would pass
    against a version that referenced it and then called ``input``.

    Two details are load-bearing and were each a failing first attempt.
    :func:`pty.fork` rather than :class:`subprocess.Popen` with a slave fd,
    because ``getpass`` opens ``/dev/tty`` and a child that is not a session
    leader resolves that to the *test runner's* terminal — where the typed
    credential never arrives, and the probe hangs. And the credential is typed
    only *after* the prompt appears, because until ``getpass`` clears ``ECHO``
    the line discipline is still echoing, and an early write would be reflected
    back by the terminal rather than by the program under test.
    """
    import pty

    typed = "typed-into-a-terminal"
    repo = str(Path(__file__).resolve().parents[2])
    script = PTY_PROBE.format(repo=repo)

    pid, master = pty.fork()
    if pid == 0:  # pragma: no cover - the child never returns to pytest
        try:
            os.execv(sys.executable, [sys.executable, "-c", script])
        finally:
            os._exit(127)

    seen = bytearray()
    failure = ""
    try:
        if not _read_until(master, b"token:", seen, budget=15.0):
            failure = f"the credential prompt never appeared: {bytes(seen)!r}"
        else:
            os.write(master, f"{typed}\n".encode())
            if not _read_until(master, b"LENGTH", seen, budget=15.0):
                failure = f"the probe never read a credential: {bytes(seen)!r}"
    finally:
        if failure:  # pragma: no cover - only when the probe wedged
            os.kill(pid, signal.SIGKILL)
        # Reaped *before* the master is closed: closing it hangs up the child's
        # session, and the child would then be reported as killed by SIGHUP
        # rather than as the clean exit it actually made.
        _, status = os.waitpid(pid, 0)
        os.close(master)

    assert not failure, failure
    text = bytes(seen).decode("utf-8", "replace")
    assert os.waitstatus_to_exitcode(status) == 0, text
    assert f"LENGTH {len(typed)}" in text
    assert typed not in text, "the terminal echoed the credential"


# ── endpoint resolution ──────────────────────────────────────────────────


def test_the_endpoint_falls_back_to_the_hermes_default() -> None:
    assert resolve_endpoint(environ={}) == "ws://127.0.0.1:9119/api/ws"


def test_a_configured_endpoint_carrying_a_credential_is_refused(tmp_path: Path) -> None:
    """The 2026-08-07 change, and the one assertion that states it directly.

    Until then this endpoint *worked*: the credential was stripped off here and
    read back as the highest-precedence credential source, so an operator who
    exported it saw nothing wrong while the value sat in their environment and
    shell history for the life of every Talaria process. Silently stripping it
    now that the route is gone would be worse than either — the configuration
    keeps working, so nothing ever tells the operator that the credential they
    exported is both exposed and useless.
    """
    with pytest.raises(CredentialError) as caught:
        resolve_endpoint(environ={GATEWAY_URL_ENV_VAR: f"ws://host:9119/api/ws?token={CANARY}&x=1"})

    message = str(caught.value)
    assert GATEWAY_URL_ENV_VAR in message, "the refusal does not say which source carried it"
    assert "refresh-credential" in message, "the refusal does not name the route that works"
    assert CANARY not in message, "the refusal reproduces the credential it is refusing"
    assert "host:9119" not in message, "the refusal echoes the URL that carried the credential"


def test_the_refusal_covers_every_shape_a_credential_arrives_in() -> None:
    """Query parameter, userinfo, and fragment — the three
    :func:`url_carries_credential` names.

    Userinfo and a fragment could never authenticate against Hermes, which reads
    the upgrade credential only from query parameters. They are refused anyway,
    for the reason the command line refuses them: both are places an operator
    puts a secret, and a rule that fired only on the form that works would send
    someone who moved their token into a fragment away happy.
    """
    for endpoint in (
        f"ws://host:9119/api/ws?token={CANARY}",
        f"ws://host:9119/api/ws?ticket={CANARY}",
        f"ws://user:{CANARY}@host:9119/api/ws",
        f"ws://host:9119/api/ws#token={CANARY}",
        f"ws://host:9119/api/ws#{CANARY}",
    ):
        assert url_carries_credential(endpoint), endpoint
        with pytest.raises(CredentialError) as caught:
            resolve_endpoint(environ={GATEWAY_URL_ENV_VAR: endpoint})
        assert CANARY not in str(caught.value), endpoint


def test_a_credential_in_the_files_url_key_is_refused_without_a_rotation_warning(
    tmp_path: Path,
) -> None:
    """Refused too, but the advice differs because the exposure does.

    A ``0600`` file is not readable by another user, so telling its owner their
    credential is exposed and must be rotated would be false. What is true is
    that an endpoint is not a credential source anywhere, so the value belongs
    in the file's ``token`` key instead.
    """
    path = tmp_path / "credentials"
    path.write_text(f'url = "ws://elsewhere:9200/api/ws?token={CANARY}"\n', encoding="utf-8")
    path.chmod(0o600)

    with pytest.raises(CredentialError) as caught:
        resolve_endpoint(environ={}, credentials_path=path)

    message = str(caught.value)
    assert "url key" in message
    assert CANARY not in message
    assert "rotate" not in message, (
        "a 0600 file is not an exposure; telling its owner to rotate would be false"
    )


def test_the_refusal_reaches_the_caller_as_a_state_not_a_traceback() -> None:
    """Same shape as every other endpoint failure (T7): a named ``problem``."""
    target = AttachTarget.from_environment(
        environ={GATEWAY_URL_ENV_VAR: f"ws://host:9119/api/ws?token={CANARY}"}
    )

    assert target.problem, "a refused endpoint produced no stated problem"
    assert target.url == ""
    assert CANARY not in target.problem
    assert CANARY not in repr(target)


def test_a_credential_free_endpoint_keeps_its_other_query_parameters() -> None:
    """The refusal must not have turned into "refuse anything with a query"."""
    resolved = resolve_endpoint(environ={GATEWAY_URL_ENV_VAR: "ws://host:9119/api/ws?x=1"})
    assert resolved == "ws://host:9119/api/ws?x=1"


def test_the_credential_file_can_supply_the_endpoint(tmp_path: Path) -> None:
    path = tmp_path / "credentials"
    path.write_text('token = "t"\nurl = "ws://elsewhere:9200/api/ws"\n', encoding="utf-8")
    path.chmod(0o600)
    assert resolve_endpoint(environ={}, credentials_path=path) == "ws://elsewhere:9200/api/ws"


def test_an_explicit_override_outranks_the_environment() -> None:
    resolved = resolve_endpoint(
        environ={GATEWAY_URL_ENV_VAR: "ws://env:1/api/ws"},
        override="ws://explicit:2/api/ws",
    )
    assert resolved == "ws://explicit:2/api/ws"


def test_attach_target_from_environment_composes_both_halves(tmp_path: Path) -> None:
    """The environment-free configuration, which is now the only supported one.

    Both halves come from the ``0600`` credential file — ``url`` for the
    endpoint, ``token`` for the credential — so nothing about this dial is
    exported, on a command line, or in shell history. This is the shape the
    README documents.
    """
    path = tmp_path / "credentials"
    path.write_text(f'token = "{CANARY}"\nurl = "ws://host:9119/api/ws"\n', encoding="utf-8")
    path.chmod(0o600)

    target = AttachTarget.from_environment(environ={}, credentials_path=path)

    assert (target.url, target.problem) == ("ws://host:9119/api/ws", "")
    assert CANARY not in target.url


def test_a_credential_file_looser_than_0600_cannot_redirect_the_dial(
    tmp_path: Path,
) -> None:
    """T4's hole: the 0600 gate guarded the token and not the endpoint.

    ``resolve_endpoint`` read the same file for its ``url`` key with no ``stat``
    at all. A world-writable credentials file therefore could not hand over a
    token — but could still point the dial at a host of the writer's choosing,
    while the token came from the environment and was delivered there intact.
    """
    path = tmp_path / "credentials"
    path.write_text('url = "ws://attacker.invalid:9119/api/ws"\n', encoding="utf-8")
    path.chmod(0o666)

    with pytest.raises(CredentialError) as caught:
        resolve_endpoint(environ={}, credentials_path=path)
    assert "0666" in str(caught.value)
    assert "attacker.invalid" not in str(caught.value)

    # And the failure reaches the caller as a state, not as a traceback.
    target = AttachTarget.from_environment(environ={}, credentials_path=path)
    assert target.problem
    assert "attacker.invalid" not in target.url
    assert target.url == ""


def test_a_0600_credential_file_still_supplies_the_endpoint(tmp_path: Path) -> None:
    """The gate above must not have broken the ordinary case."""
    path = tmp_path / "credentials"
    path.write_text('url = "ws://elsewhere:9200/api/ws"\n', encoding="utf-8")
    path.chmod(0o600)
    target = AttachTarget.from_environment(environ={}, credentials_path=path)
    assert (target.url, target.problem) == ("ws://elsewhere:9200/api/ws", "")


# ── a malformed endpoint is an outcome, not an exception (T7) ────────────


#: ``urlsplit`` raises ``ValueError("Invalid IPv6 URL")`` on this: the bracket
#: opens an IPv6 literal that never closes.
MALFORMED_ENDPOINT = "ws://[bad::/api/ws"


def test_a_malformed_endpoint_becomes_a_target_problem_not_a_value_error() -> None:
    """The module's shape is outcome-not-exception; this was the one exception."""
    target = AttachTarget.from_url(f"{MALFORMED_ENDPOINT}?token={CANARY}")

    assert target.problem, "a malformed endpoint produced no stated problem"
    assert target.url == ""
    assert CANARY not in target.problem
    assert CANARY not in target.safe_url
    assert CANARY not in repr(target)


@pytest.mark.asyncio
async def test_a_malformed_endpoint_reaches_the_caller_as_a_connect_failure() -> None:
    target = AttachTarget.from_url(MALFORMED_ENDPOINT)
    outcome = await attach(target, Credential("token", CANARY, "file"))

    assert isinstance(outcome, AttachFailure)
    assert outcome.kind == "connect_failed"
    assert "not a valid URL" in outcome.detail
    assert_outcome_withholds(outcome, CANARY)


def test_a_malformed_endpoint_in_the_environment_is_named_not_raised() -> None:
    with pytest.raises(CredentialError) as caught:
        resolve_endpoint(environ={GATEWAY_URL_ENV_VAR: f"{MALFORMED_ENDPOINT}?token={CANARY}"})
    assert GATEWAY_URL_ENV_VAR in str(caught.value)
    assert CANARY not in str(caught.value)

    target = AttachTarget.from_environment(
        environ={GATEWAY_URL_ENV_VAR: f"{MALFORMED_ENDPOINT}?token={CANARY}"}
    )
    assert target.problem
    assert CANARY not in target.problem


def test_userinfo_is_withheld_from_a_shown_endpoint() -> None:
    """``redact_url`` covers userinfo; ``safe_url`` inherits that rather than
    re-deriving it, so the two cannot disagree."""
    target = AttachTarget.from_url(f"ws://user:{CANARY}@host:9119/api/ws")
    assert CANARY not in target.safe_url


# ── The endpoint fragment, 2026-08-05 ──────────────────────────────────
#
# The command line refuses an endpoint carrying a credential in its fragment
# (``talaria/cli.py``). These pin the invariant underneath that refusal: every
# other way an endpoint arrives — ``TALARIA_GATEWAY_URL``, the credential
# file's ``url`` key, a direct ``from_url`` — went through
# ``strip_credential_query``, which read query keys and wrote the fragment back
# out unread.


def test_a_fragment_is_dropped_from_a_configured_endpoint() -> None:
    """Whole, and without being read. A fragment has no key to match against
    ``CREDENTIAL_QUERY_KEYS``, and a WebSocket endpoint has no use for one."""
    assert strip_credential_query(f"ws://127.0.0.1:9119/api/ws#token={CANARY}") == (
        "ws://127.0.0.1:9119/api/ws"
    )

    target = AttachTarget.from_url(f"ws://127.0.0.1:9119/api/ws#token={CANARY}")
    assert CANARY not in target.url
    assert CANARY not in target.safe_url
    assert CANARY not in repr(target)


def test_a_fragment_is_dropped_whatever_it_looks_like() -> None:
    """``#token=v`` and ``#v`` are equally ordinary ways to write a fragment, so
    a rule that fired only on the first would miss the second."""
    for fragment in (f"token={CANARY}", CANARY, f"a=1&ticket={CANARY}"):
        target = AttachTarget.from_url(f"ws://127.0.0.1:9119/api/ws#{fragment}")
        assert CANARY not in target.url, fragment


def test_the_endpoint_fragment_from_the_environment_is_now_refused() -> None:
    """These two sources are no longer behind the invariant; they are refused.

    Until 2026-08-07 ``TALARIA_GATEWAY_URL`` and the credential file's ``url``
    key were the two sources the command-line refusal did not stand in front of,
    so the most this pair could assert was that the fragment reached no surface.
    Both are now refused outright, which is strictly stronger — and the fragment
    still reaches no surface, because a refused endpoint yields an empty
    :attr:`~talaria.transport.attach.AttachTarget.url`.
    """
    target = AttachTarget.from_environment(
        environ={GATEWAY_URL_ENV_VAR: f"ws://127.0.0.1:9119/api/ws#token={CANARY}"}
    )

    assert target.problem
    assert target.url == ""
    assert CANARY not in target.problem
    assert CANARY not in target.safe_url


def test_the_endpoint_fragment_from_the_credential_file_is_now_refused(
    tmp_path: Path,
) -> None:
    """The other one: the ``url`` key in the credential file."""
    path = tmp_path / "credentials"
    path.write_text(f'url = "ws://127.0.0.1:9119/api/ws#token={CANARY}"\n', encoding="utf-8")
    path.chmod(0o600)

    target = AttachTarget.from_environment(environ={}, credentials_path=path)

    assert target.problem
    assert target.url == ""
    assert CANARY not in target.problem
    assert CANARY not in target.safe_url


def test_stripping_a_fragment_stays_the_invariant_under_the_refusal() -> None:
    """The refusal is a gate on configured endpoints; this is the floor.

    :class:`~talaria.transport.attach.AttachTarget` can be constructed directly,
    and :meth:`from_url` still strips rather than refuses, so the guarantee that
    no fragment survives into a dialled URL cannot rest on the refusal alone.
    """
    assert strip_credential_query(f"ws://127.0.0.1:9119/api/ws#token={CANARY}") == (
        "ws://127.0.0.1:9119/api/ws"
    )
    assert CANARY not in AttachTarget.from_url(f"ws://127.0.0.1:9119/api/ws#{CANARY}").url


def test_a_dialled_url_carries_no_fragment() -> None:
    """``dial_url`` rebuilt the URL from its own ``urlsplit``, so a fragment on a
    directly-constructed target would ride onto the one URL that is dialled."""
    target = AttachTarget(url=f"ws://127.0.0.1:9119/api/ws#{CANARY}")
    dial = target.dial_url(Credential("token", "fresh", "file"))

    assert "#" not in dial
    assert CANARY not in dial


def test_dropping_a_fragment_leaves_the_rest_of_the_url_byte_identical() -> None:
    """The query is re-encoded only when a parameter actually leaves it.

    Routing every fragment-bearing URL through ``urlencode`` would normalize
    percent-escapes in a query that had nothing wrong with it — the bytes KTD6
    compares against the TypeScript reference.
    """
    assert strip_credential_query("ws://127.0.0.1:9119/api/ws?x=%2Ffoo#frag") == (
        "ws://127.0.0.1:9119/api/ws?x=%2Ffoo"
    )


def test_the_recorded_endpoint_header_withholds_a_fragment(tmp_path: Path) -> None:
    """R9 again, through the real recorder rather than the redactor.

    ``FrameRecorder`` is handed the endpoint as a string, so this covers the
    path where a caller never built an ``AttachTarget`` at all.
    """
    log = tmp_path / "frames.jsonl"
    recorder = FrameRecorder(log, f"ws://127.0.0.1:9119/api/ws#token={CANARY}")
    recorder.close()

    text = log.read_text(encoding="utf-8")
    assert CANARY not in text
    assert "redacted" in json.loads(text.splitlines()[0])["endpoint"]
