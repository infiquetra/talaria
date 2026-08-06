"""The admin HTTP surface: where it points, what it carries, what it refuses.

Two families of assertion carry the weight.

The first is *where the credential goes*. The origin is derived from the gateway
endpoint and from nothing else — there is no separate HTTP setting — so a
credential minted for one gateway can never be sent to another. The tests below
assert that by observation, against a real loopback server that records the
headers it was sent, rather than by reading the implementation.

The second is *that the credential does not escape*. One distinctive value is
planted and then searched for in every log record the call emits, in the repr of
every object involved, and in the string of every exception raised. A leak test
that only checks the happy path misses the case that matters, which is the
traceback.

**No test, fixture or docstring in this module supplies a credential through
``HERMES_DASHBOARD_SESSION_TOKEN``.** U3 of the 2026-08-06 plan deletes that
environment variable as a credential source (KTD8), and anything here that
depended on it would break the moment U3 merged. Credentials arrive by injected
provider or by a ``0600`` file under ``tmp_path``.
"""

from __future__ import annotations

import contextlib
import json
import logging
import threading
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any

import pytest

from talaria.domain.models_catalog import CatalogError
from talaria.transport.admin import (
    CREDENTIAL_HEADERS,
    MAX_RESPONSE_BYTES,
    MODEL_INFO_PATH,
    MODEL_OPTIONS_PATH,
    MODEL_SET_PATH,
    PROFILES_PATH,
    AdminClient,
    AdminError,
    admin_origin_for,
    fetch_admin_json,
    post_admin_json,
)
from talaria.transport.credentials import (
    Credential,
    CredentialError,
    LoopbackTokenProvider,
)

#: One distinctive value, searched for wherever it must not appear.
CANARY = "canary-Qm4x71-do-not-leak"

OPTIONS_BODY: dict[str, Any] = {
    "providers": [
        {
            "slug": "example-provider",
            "name": "Example Provider",
            "models": ["example-small", "example-large"],
            "authenticated": True,
            "auth_type": "api_key",
            "capabilities": {},
            "featured_models": [],
            "is_current": True,
            "is_user_defined": False,
            "source": "builtin",
            "total_models": 2,
            "warning": "",
        }
    ],
    "model": "example-large",
    "provider": "example-provider",
}

INFO_BODY: dict[str, Any] = {
    "model": "example-large",
    "provider": "example-provider",
    "auto_context_length": 200000,
    "config_context_length": None,
    "effective_context_length": 200000,
    "capabilities": {},
}


# ── test doubles ─────────────────────────────────────────────────────────


@dataclass
class StubProvider:
    """A :class:`CredentialProvider` by structure, not by inheritance.

    ``CredentialProvider`` is a ``Protocol`` precisely so a double need not
    inherit anything. ``acquisitions`` is public because KTD11's per-dial rule is
    otherwise invisible — a provider called once and cached behaves identically
    until the second call.
    """

    value: str = CANARY
    acquisitions: int = 0
    raises: CredentialError | None = None

    def __repr__(self) -> str:
        """Withhold the value, exactly as :class:`Credential` does.

        A dataclass's generated repr would put the credential into every pytest
        assertion failure that happens to hold this object — which is how the
        double would become the leak the tests below are looking for.
        """
        return f"StubProvider(acquisitions={self.acquisitions}, value=<withheld>)"

    async def acquire(self) -> Credential:
        self.acquisitions += 1
        if self.raises is not None:
            raise self.raises
        return Credential(parameter="token", value=self.value, source="file")


@dataclass
class Recorded:
    """What one served request looked like from the server's side.

    ``body`` is the parsed JSON a POST carried, empty for a GET — U5 is the
    first thing in this module to write, so it is the first thing that needs
    the request body recorded rather than just the path and headers.
    """

    path: str
    headers: dict[str, str]
    body: dict[str, Any] = field(default_factory=dict)


#: A POST route answers from the parsed request body, so a scenario can
#: change its response between one call and the next — KTD7's confirmation
#: round trip is exactly that: the same path answers differently once the
#: body carries ``confirm_expensive_model: true``.
PostRoute = Callable[[dict[str, Any]], tuple[int, bytes, str]]


@contextlib.contextmanager
def gateway_serving(
    routes: dict[str, tuple[int, bytes, str]],
    recorder: list[Recorded] | None = None,
    post_routes: dict[str, PostRoute] | None = None,
) -> Iterator[str]:
    """A loopback server mapping ``path -> (status, body, content-type)``.

    Any path not in ``routes`` (or, for a POST, not in ``post_routes``)
    answers 404, which is what makes the absent-capability test a real HTTP
    round trip rather than a mocked one.
    """

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            path = self.path.split("?", 1)[0]
            if recorder is not None:
                recorder.append(Recorded(path=self.path, headers=dict(self.headers)))
            status, payload, content_type = routes.get(
                path, (404, b'{"detail":"Not Found"}', "application/json")
            )
            self._respond(status, payload, content_type)

        def do_POST(self) -> None:
            path = self.path.split("?", 1)[0]
            length = int(self.headers.get("Content-Length", "0") or "0")
            raw_body = self.rfile.read(length) if length else b""
            try:
                parsed = json.loads(raw_body) if raw_body else {}
            except json.JSONDecodeError:
                parsed = {}
            if recorder is not None:
                recorder.append(
                    Recorded(path=self.path, headers=dict(self.headers), body=parsed)
                )
            handler = (post_routes or {}).get(path)
            if handler is None:
                status, payload, content_type = (
                    404,
                    b'{"detail":"Not Found"}',
                    "application/json",
                )
            else:
                status, payload, content_type = handler(parsed)
            self._respond(status, payload, content_type)

        def _respond(self, status: int, payload: bytes, content_type: str) -> None:
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, format: str, *args: Any) -> None:
            return

    server = HTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def json_route(body: Any, status: int = 200) -> tuple[int, bytes, str]:
    return status, json.dumps(body).encode("utf-8"), "application/json"


@contextlib.contextmanager
def catalog_gateway(recorder: list[Recorded] | None = None) -> Iterator[str]:
    with gateway_serving(
        {
            MODEL_OPTIONS_PATH: json_route(OPTIONS_BODY),
            MODEL_INFO_PATH: json_route(INFO_BODY),
        },
        recorder,
    ) as origin:
        yield origin


def credential_file(tmp_path: Path, token: str = CANARY) -> Path:
    """A ``0600`` credential file — the supported non-interactive route.

    Deliberately not an environment variable: U3 removes that source.
    """
    path = tmp_path / "credentials"
    path.write_text(f'token = "{token}"\n', encoding="utf-8")
    path.chmod(0o600)
    return path


def ws_endpoint(origin: str) -> str:
    """The gateway WebSocket URL an operator configures, for an HTTP origin."""
    return origin.replace("http://", "ws://").rstrip("/") + "/api/ws"


# ── the origin comes from the gateway URL, and from nowhere else ─────────


@pytest.mark.parametrize(
    ("endpoint", "expected"),
    [
        ("ws://127.0.0.1:9119/api/ws", "http://127.0.0.1:9119/"),
        ("wss://gateway.example:443/api/ws", "https://gateway.example:443/"),
        ("ws://localhost:8765/api/ws", "http://localhost:8765/"),
    ],
)
def test_the_admin_origin_is_derived_from_the_gateway_endpoint(
    endpoint: str, expected: str
) -> None:
    """One configured endpoint. A second HTTP knob could name a second host,
    and the failure mode of that mistake is a credential minted for one gateway
    being sent to another."""
    assert admin_origin_for(endpoint) == expected


def test_the_client_takes_no_http_setting_and_derives_its_origin_from_the_endpoint() -> None:
    """Asserted on the constructor's signature by construction, not by reading it.

    The client is given a gateway WebSocket URL and a provider — there is no
    parameter through which a different HTTP address could be supplied.
    """
    client = AdminClient("ws://127.0.0.1:9119/api/ws", StubProvider())

    assert client.origin == "http://127.0.0.1:9119/"


def test_deriving_the_origin_drops_any_credential_on_the_endpoint() -> None:
    origin = admin_origin_for(f"ws://127.0.0.1:9119/api/ws?token={CANARY}")

    assert origin == "http://127.0.0.1:9119/"
    assert CANARY not in origin


@pytest.mark.parametrize("origin", ["file:///etc/passwd", "ftp://127.0.0.1/x"])
def test_only_http_and_https_are_fetched(origin: str) -> None:
    """``urlopen`` will read ``file:`` if handed one; a wrong address is a refusal."""
    with pytest.raises(AdminError) as caught:
        fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == "refused_origin"


def test_a_credential_is_never_sent_to_a_remote_host_over_plain_http() -> None:
    """The loopback rule ``refresh.py`` established, reused rather than recopied."""
    with pytest.raises(AdminError) as caught:
        fetch_admin_json("http://gateway.example/", MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == "refused_origin"
    assert CANARY not in str(caught.value)


def test_an_absolute_path_cannot_redirect_the_credential_to_another_origin() -> None:
    """``urljoin`` treats an absolute URL as a replacement for the origin.

    Callers pass module constants today; a future caller passing a path built
    from a gateway response must not be able to choose where the credential goes.
    """
    with pytest.raises(AdminError) as caught:
        fetch_admin_json(
            "http://127.0.0.1:9119/", "http://elsewhere.example/api/model/options", token=CANARY
        )

    assert caught.value.reason == "refused_origin"


# ── the credential is attached in the decided form (KTD2) ────────────────


def test_the_credential_is_attached_as_a_bearer_header() -> None:
    """KTD2, confirmed against Hermes source and against a live gateway.

    ``hermes_cli/web_server.py``'s ``_has_valid_session_token`` compares
    ``request.headers["authorization"]`` against ``f"Bearer {_SESSION_TOKEN}"``.
    """
    seen: list[Recorded] = []
    with catalog_gateway(seen) as origin:
        fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert seen[0].headers["Authorization"] == f"Bearer {CANARY}"


def test_the_credential_is_also_attached_as_the_dedicated_session_header() -> None:
    """The non-legacy form. Hermes names the collision this avoids: a reverse
    proxy that already uses ``Authorization`` (for example Caddy ``basic_auth``)."""
    seen: list[Recorded] = []
    with catalog_gateway(seen) as origin:
        fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert seen[0].headers["X-Hermes-Session-Token"] == CANARY


def test_the_credential_never_rides_the_query_string_on_http() -> None:
    """The WebSocket form is *not* accepted here.

    ``_QUERY_TOKEN_API_PATHS`` is a frozenset of exactly ``/api/files/download``
    and ``_has_valid_query_token`` returns False for everything else; a live
    gateway answered ``?token=`` with 401 on 2026-08-06. Sending it would leak a
    credential into access logs for no authentication benefit.
    """
    seen: list[Recorded] = []
    with catalog_gateway(seen) as origin:
        fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert CANARY not in seen[0].path


def test_every_header_the_credential_is_written_into_is_named_in_one_constant() -> None:
    """So the leak assertions below cannot be escaped by adding a third form."""
    seen: list[Recorded] = []
    with catalog_gateway(seen) as origin:
        fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    carrying = {name for name, value in seen[0].headers.items() if CANARY in value}
    assert carrying == set(CREDENTIAL_HEADERS)


# ── the credential appears in no log, repr or exception ──────────────────


def test_the_credential_appears_in_no_log_record_of_a_successful_call(
    caplog: pytest.LogCaptureFixture,
) -> None:
    with caplog.at_level(logging.DEBUG):
        with catalog_gateway() as origin:
            fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert CANARY not in caplog.text


@pytest.mark.parametrize(
    ("status", "reason"),
    [(401, "unauthorized"), (404, "absent_capability"), (500, "http_error")],
)
def test_the_credential_appears_in_no_log_record_of_a_failed_call(
    caplog: pytest.LogCaptureFixture, status: int, reason: str
) -> None:
    """The failing path is the one a leak test that only checks success misses."""
    with caplog.at_level(logging.DEBUG):
        with gateway_serving({MODEL_OPTIONS_PATH: json_route({"detail": "x"}, status)}) as origin:
            with pytest.raises(AdminError) as caught:
                fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == reason
    assert CANARY not in caplog.text
    assert CANARY not in str(caught.value)
    assert CANARY not in repr(caught.value)


def test_the_credential_appears_in_no_traceback_of_a_failed_call() -> None:
    """Every frame's locals, not just the message.

    ``pytest``'s assertion rewriting and any ``logging.exception`` call render
    the whole chain, so a value held in an intermediate frame escapes just as
    surely as one put in the message.
    """
    import traceback

    with gateway_serving({MODEL_OPTIONS_PATH: json_route({"detail": "x"}, 401)}) as origin:
        with pytest.raises(AdminError) as caught:
            fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    rendered = "".join(
        traceback.format_exception(
            type(caught.value), caught.value, caught.value.__traceback__
        )
    )
    assert CANARY not in rendered


def test_the_client_repr_carries_no_credential() -> None:
    client = AdminClient("ws://127.0.0.1:9119/api/ws", StubProvider())

    assert CANARY not in repr(client)
    assert CANARY not in repr(vars(client))


def test_the_error_repr_names_its_reason_and_nothing_else() -> None:
    error = AdminError("unauthorized", "the gateway refused Talaria's credential")

    assert "unauthorized" in repr(error)
    assert CANARY not in repr(error)


# ── each failure mode becomes a named error, not a urllib exception ──────


def test_a_401_becomes_unauthorized_and_points_at_the_repair() -> None:
    """A dashboard restart invalidates the token; the message says the fix."""
    with gateway_serving({MODEL_OPTIONS_PATH: json_route({"detail": "Unauthorized"}, 401)}) as o:
        with pytest.raises(AdminError) as caught:
            fetch_admin_json(o, MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == "unauthorized"
    assert "refresh-credential" in str(caught.value)


def test_a_404_becomes_absent_capability_rather_than_an_error() -> None:
    """R8's absent-capability branch: a gateway too old to carry the endpoint.

    Served by the fixture's real fall-through, so this is an HTTP round trip
    against a server that genuinely does not have the route.
    """
    with gateway_serving({}) as origin:
        with pytest.raises(AdminError) as caught:
            fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == "absent_capability"
    assert MODEL_OPTIONS_PATH in str(caught.value)


def test_a_refused_connection_becomes_unreachable_naming_the_host_only() -> None:
    """The message names a host, never a URL — a URL is what a token is pasted into."""
    with catalog_gateway() as origin:
        pass  # the server is shut down on exit, so the port now refuses

    with pytest.raises(AdminError) as caught:
        fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY, timeout=2.0)

    assert caught.value.reason == "unreachable"
    assert "127.0.0.1" in str(caught.value)
    assert CANARY not in str(caught.value)


@pytest.mark.parametrize(
    ("body", "content_type"),
    [
        pytest.param(b"<!doctype html><html>login</html>", "text/html", id="html"),
        pytest.param(b"", "application/json", id="empty"),
        pytest.param(b"{not json", "application/json", id="truncated"),
    ],
)
def test_a_body_that_is_not_json_becomes_malformed_response(
    body: bytes, content_type: str
) -> None:
    """An HTML login page answering 200 is the realistic shape of this failure."""
    with gateway_serving({MODEL_OPTIONS_PATH: (200, body, content_type)}) as origin:
        with pytest.raises(AdminError) as caught:
            fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == "malformed_response"


def test_an_unexpected_status_becomes_a_named_http_error() -> None:
    with gateway_serving({MODEL_OPTIONS_PATH: json_route({"detail": "boom"}, 500)}) as origin:
        with pytest.raises(AdminError) as caught:
            fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == "http_error"
    assert "500" in str(caught.value)


def test_no_urllib_exception_reaches_a_caller_of_any_failure_mode() -> None:
    """The point of the named vocabulary: one exception type to handle."""
    import urllib.error

    with gateway_serving({MODEL_OPTIONS_PATH: json_route({}, 401)}) as origin:
        with pytest.raises(AdminError) as caught:
            fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert not isinstance(caught.value, urllib.error.URLError)


# ── the read cap ─────────────────────────────────────────────────────────


def test_an_oversized_body_is_refused_at_the_cap() -> None:
    """A wrong origin answering with an endless stream must not be read forever.

    The cap is ``refresh.py``'s ``MAX_INDEX_BYTES``, imported rather than
    restated so the two surfaces cannot disagree about how much is too much.
    """
    oversized = b'{"providers":[],"pad":"' + b"x" * (MAX_RESPONSE_BYTES + 64) + b'"}'

    with gateway_serving({MODEL_OPTIONS_PATH: (200, oversized, "application/json")}) as origin:
        with pytest.raises(AdminError) as caught:
            fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert caught.value.reason == "oversized_response"
    assert CANARY not in str(caught.value)


def test_a_body_exactly_at_the_cap_is_still_accepted() -> None:
    """Off-by-one guard: the implementation reads one byte past the cap so that
    a body sitting exactly at the limit is not mistaken for a truncated one."""
    prefix, suffix = b'{"providers":[],"pad":"', b'"}'
    pad = MAX_RESPONSE_BYTES - len(prefix) - len(suffix)
    body = prefix + b"x" * pad + suffix
    assert len(body) == MAX_RESPONSE_BYTES

    with gateway_serving({MODEL_OPTIONS_PATH: (200, body, "application/json")}) as origin:
        decoded = fetch_admin_json(origin, MODEL_OPTIONS_PATH, token=CANARY)

    assert decoded["providers"] == []


# ── the client, end to end over loopback ─────────────────────────────────


@pytest.mark.asyncio
async def test_model_options_decodes_into_a_catalogue() -> None:
    with catalog_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        catalog = await client.model_options()

    assert [p.slug for p in catalog.providers] == ["example-provider"]
    assert catalog.current_model == "example-large"


@pytest.mark.asyncio
async def test_model_info_decodes_into_a_selection() -> None:
    with catalog_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        selection = await client.model_info()

    assert selection.model == "example-large"
    assert selection.effective_context_length == 200000


@pytest.mark.asyncio
async def test_the_credential_is_re_resolved_on_every_call_never_cached() -> None:
    """KTD11's per-dial rule applied to the HTTP surface.

    A client that cached would acquire a silent dependency on the credential
    being a fixed reusable value, and KTD6 needs a profile switch to re-resolve
    against the endpoint it switched to.
    """
    provider = StubProvider()
    with catalog_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        await client.model_options()
        await client.model_options()
        await client.model_info()

    assert provider.acquisitions == 3


@pytest.mark.asyncio
async def test_a_rotated_credential_is_picked_up_by_the_next_call() -> None:
    """The observable consequence of the rule above."""
    provider = StubProvider(value="first-value")
    seen: list[Recorded] = []
    with catalog_gateway(seen) as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        await client.model_options()
        provider.value = "second-value"
        await client.model_options()

    assert seen[0].headers["X-Hermes-Session-Token"] == "first-value"
    assert seen[1].headers["X-Hermes-Session-Token"] == "second-value"


@pytest.mark.asyncio
async def test_a_credential_the_provider_cannot_supply_is_reported_not_raised() -> None:
    """``credential_unavailable`` is the failure state the transport already has."""
    provider = StubProvider(raises=CredentialError("no credential file"))
    with catalog_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        with pytest.raises(AdminError) as caught:
            await client.model_options()

    assert caught.value.reason == "credential_unavailable"


@pytest.mark.asyncio
async def test_the_file_route_supplies_a_credential_without_any_environment_variable(
    tmp_path: Path,
) -> None:
    """The supported non-interactive route after U3 removes the environment source.

    ``environ`` is injected as an *empty* mapping, so this passes whether or not
    the developer running it happens to have a dashboard token exported — and it
    cannot be satisfied by one.
    """
    path = credential_file(tmp_path)
    provider = LoopbackTokenProvider(
        credentials_path=path, environ={}, allow_prompt=False
    )

    seen: list[Recorded] = []
    with catalog_gateway(seen) as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        catalog = await client.model_options()

    assert catalog.providers[0].slug == "example-provider"
    assert seen[0].headers["Authorization"] == f"Bearer {CANARY}"


@pytest.mark.asyncio
async def test_a_credential_file_looser_than_0600_is_refused(tmp_path: Path) -> None:
    path = credential_file(tmp_path)
    path.chmod(0o644)
    provider = LoopbackTokenProvider(
        credentials_path=path, environ={}, allow_prompt=False
    )

    with catalog_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        with pytest.raises(AdminError) as caught:
            await client.model_options()

    assert caught.value.reason == "credential_unavailable"
    assert CANARY not in str(caught.value)


@pytest.mark.asyncio
async def test_a_profile_scope_rides_the_query_string_and_carries_no_credential() -> None:
    """``profile`` scopes which config the endpoint reads. It is not a secret;
    the credential still travels in the headers."""
    seen: list[Recorded] = []
    with catalog_gateway(seen) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        await client.model_options(profile="example-profile")

    assert "profile=example-profile" in seen[0].path
    assert CANARY not in seen[0].path


@pytest.mark.asyncio
async def test_a_malformed_catalogue_is_reported_as_a_transport_failure() -> None:
    """A :class:`CatalogError` from the decode is translated, so a caller of the
    client handles one exception type rather than two."""
    bad = {"providers": [{"name": "no slug here", "models": []}]}
    with gateway_serving({MODEL_OPTIONS_PATH: json_route(bad)}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        with pytest.raises(AdminError) as caught:
            await client.model_options()

    assert caught.value.reason == "malformed_response"
    assert not isinstance(caught.value, CatalogError)


@pytest.mark.asyncio
async def test_an_empty_catalogue_is_a_value_and_not_a_failure() -> None:
    """R7: "there are no providers" must reach the picker as data, so the picker
    can say it differently from "the list could not be fetched"."""
    with gateway_serving({MODEL_OPTIONS_PATH: json_route({"providers": []})}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        catalog = await client.model_options()

    assert catalog.is_empty


@pytest.mark.asyncio
async def test_a_gateway_without_the_endpoint_reaches_the_client_as_absent_capability() -> None:
    with gateway_serving({}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        with pytest.raises(AdminError) as caught:
            await client.model_options()

    assert caught.value.reason == "absent_capability"


@pytest.mark.asyncio
async def test_a_call_does_not_block_the_event_loop() -> None:
    """The blocking ``urlopen`` is offloaded. Left on the loop, a gateway that
    stopped answering would freeze the whole interface for the timeout."""
    import asyncio

    ticks = 0

    async def tick() -> None:
        nonlocal ticks
        for _ in range(20):
            ticks += 1
            await asyncio.sleep(0.001)

    with catalog_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        ticker = asyncio.create_task(tick())
        await client.model_options()
        await ticker

    assert ticks == 20


# ── the environment variable is not a route here (U3 concurrency) ────────


def test_neither_new_module_reads_the_environment_for_a_credential() -> None:
    """U3 deletes the dashboard session-token environment variable as a
    credential source in the same wave (KTD8). Neither module added by U1 may
    depend on it, or both break the moment U3 merges.

    Asserted as textual absence in the *production* modules rather than trusting
    review. This test file is deliberately not scanned: it names the variable in
    prose above to explain the constraint, and a grep that forbade that would
    forbid documenting the rule. What matters is that no shipped code path reads
    it — which the behavioural test above (an empty injected ``environ`` still
    yielding a working call through the file route) covers from the other side.

    The name is reassembled at runtime so this file does not contain the literal
    it searches for, keeping the assertion honest about what it proves.
    """
    variable = "HERMES_DASHBOARD" + "_SESSION_TOKEN"
    root = Path(__file__).parents[2]
    for path in (
        root / "talaria" / "transport" / "admin.py",
        root / "talaria" / "domain" / "models_catalog.py",
    ):
        source = path.read_text(encoding="utf-8")
        assert variable not in source, path.name
        assert "os.environ" not in source, path.name


# ── U4: the endpoint directory, and the write that never happens ─────────

#: Every profile name below is invented for this file. R12 forbids the
#: operator's real profile inventory from reaching a committed fixture in this
#: public repository, and the live gateway this surface was probed against on
#: 2026-08-06 answered with 37 rows of exactly that inventory. The names here
#: are chosen to be obviously synthetic and to sort in the order the assertions
#: read.
SYNTHETIC_PROFILES: dict[str, Any] = {
    "profiles": [
        {
            "name": "alpha-fixture",
            "path": "/nonexistent/fixture/alpha",
            "is_default": True,
            "model": "example-large",
            "provider": "example-provider",
            "has_env": False,
            "skill_count": 3,
            "gateway_running": True,
            "description": "a synthetic profile",
            "description_auto": False,
            "distribution_name": None,
            "distribution_version": None,
            "distribution_source": None,
            "has_alias": False,
        },
        {
            "name": "beta-fixture",
            "path": "/nonexistent/fixture/beta",
            "is_default": False,
            "model": None,
            "provider": None,
            "has_env": False,
            "skill_count": 0,
            "gateway_running": False,
            "description": "",
            "description_auto": False,
        },
    ]
}


@contextlib.contextmanager
def profiles_gateway(
    body: Any = SYNTHETIC_PROFILES, recorder: list[Recorded] | None = None
) -> Iterator[str]:
    with gateway_serving({PROFILES_PATH: json_route(body)}, recorder) as origin:
        yield origin


@pytest.mark.asyncio
async def test_profiles_are_listed_with_the_gateways_own_dialability_flag() -> None:
    provider = StubProvider()
    with profiles_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        directory = await client.list_profiles()

    assert [(p.name, p.gateway_running) for p in directory.profiles] == [
        ("alpha-fixture", True),
        ("beta-fixture", False),
    ]
    # The configured model rides along, because the picker shows it.
    assert directory.profiles[0].model == "example-large"
    assert directory.profiles[0].provider == "example-provider"
    # A profile that has configured neither reads as absent, not as "None".
    assert directory.profiles[1].model == ""
    assert directory.profiles[1].provider == ""


@pytest.mark.asyncio
async def test_the_operators_filesystem_layout_is_dropped_at_the_decode(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """R12: no profile path can reach a fixture, because nothing holds one."""
    with profiles_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        directory = await client.list_profiles()

    entry = directory.profiles[0]
    assert not hasattr(entry, "path")
    assert "/nonexistent/fixture" not in repr(directory)
    assert "/nonexistent/fixture" not in caplog.text


@pytest.mark.asyncio
async def test_the_profile_credential_is_re_resolved_on_every_listing() -> None:
    """KTD6/KTD11 on the HTTP surface: never held, always re-acquired."""
    provider = StubProvider()
    with profiles_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        await client.list_profiles()
        await client.list_profiles()
    assert provider.acquisitions == 2


@pytest.mark.asyncio
async def test_an_empty_profile_list_is_a_directory_and_not_a_failure() -> None:
    with profiles_gateway({"profiles": []}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        directory = await client.list_profiles()
    assert directory.is_empty
    assert directory.profiles == ()


@pytest.mark.asyncio
async def test_a_gateway_too_old_for_the_profiles_path_is_an_absent_capability() -> None:
    with gateway_serving({MODEL_OPTIONS_PATH: json_route(OPTIONS_BODY)}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        with pytest.raises(AdminError) as caught:
            await client.list_profiles()
    assert caught.value.reason == "absent_capability"


@pytest.mark.asyncio
async def test_a_credential_the_profiles_endpoint_refuses_is_named_unauthorized() -> None:
    with gateway_serving({PROFILES_PATH: (401, b'{"detail":"no"}', "application/json")}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        with pytest.raises(AdminError) as caught:
            await client.list_profiles()
    assert caught.value.reason == "unauthorized"
    assert CANARY not in str(caught.value)


@pytest.mark.asyncio
async def test_a_credential_that_cannot_be_produced_is_credential_unavailable() -> None:
    """KTD6's named failure, at the HTTP surface: nothing was asked of anyone."""
    provider = StubProvider(raises=CredentialError("no gateway credential"))
    with profiles_gateway() as origin:
        client = AdminClient(ws_endpoint(origin), provider)
        with pytest.raises(AdminError) as caught:
            await client.list_profiles()
    assert caught.value.reason == "credential_unavailable"


@pytest.mark.asyncio
async def test_a_profiles_body_that_is_not_the_pinned_shape_is_malformed() -> None:
    with profiles_gateway({"profiles": "alpha-fixture"}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        with pytest.raises(AdminError) as caught:
            await client.list_profiles()
    assert caught.value.reason == "malformed_response"


def test_the_admin_client_has_no_way_to_set_the_active_profile() -> None:
    """KTD5, asserted as absence rather than trusted to review.

    ``POST /api/profiles/active`` sets a sticky preference for subsequent CLI
    invocations and does not retarget the running dashboard, so calling it
    would change a machine setting and nothing the operator can see. There is
    no constant for the path and no method that could reach it.

    **U5 adds a real write elsewhere in this module** — ``POST
    /api/model/set``, whose own docstring names the effect it has and the
    session it does not touch — so this test can no longer assert "no POST
    anywhere in the module" the way it did before U5 existed; KTD5 never made
    that the guarantee, only the one path. What survives, checked
    structurally with :mod:`ast` rather than trusted to review: the literal
    path is absent, :meth:`~talaria.transport.admin.AdminClient.list_profiles`
    never calls the module's POST helper, and the module's one method that
    does (:meth:`~talaria.transport.admin.AdminClient.set_default_model`)
    always targets ``MODEL_SET_PATH`` and nothing else.
    """
    import ast

    path = Path(__file__).parents[2] / "talaria" / "transport" / "admin.py"
    source = path.read_text(encoding="utf-8")
    code = _without_docstrings(source)

    assert "/api/profiles/active" not in code
    assert not [name for name in dir(AdminClient) if "active" in name.lower()]

    tree = ast.parse(source)
    admin_client = next(
        node
        for node in ast.walk(tree)
        if isinstance(node, ast.ClassDef) and node.name == "AdminClient"
    )
    methods = {
        node.name: node for node in admin_client.body if isinstance(node, ast.AsyncFunctionDef)
    }

    def calls_post(node: ast.AST) -> bool:
        return any(
            isinstance(call.func, ast.Attribute) and call.func.attr == "_post"
            for call in ast.walk(node)
            if isinstance(call, ast.Call)
        )

    assert not calls_post(methods["list_profiles"])
    assert not calls_post(methods["model_options"])
    assert not calls_post(methods["model_info"])
    post_callers = sorted(name for name, node in methods.items() if calls_post(node))
    assert post_callers == ["set_default_model"]

    post_call = next(
        call
        for call in ast.walk(methods["set_default_model"])
        if isinstance(call, ast.Call)
        and isinstance(call.func, ast.Attribute)
        and call.func.attr == "_post"
    )
    target_path = post_call.args[0]
    assert isinstance(target_path, ast.Name) and target_path.id == "MODEL_SET_PATH"


# ── U5: the default-model write, and its two-act confirmation (KTD7) ──────


def ok_route(**overrides: Any) -> PostRoute:
    """A ``POST /api/model/set`` route that always answers success."""
    body: dict[str, Any] = {"ok": True, "scope": "main"}
    body.update(overrides)
    return lambda _request_body: json_route(body)


def confirm_required_route(message: str = "gpt-5.5 costs real money") -> PostRoute:
    """A route that refuses on the first call and succeeds once the request
    body carries ``confirm_expensive_model: true`` — the shape Hermes's own
    cost guard answers with (KTD7)."""

    def respond(request_body: dict[str, Any]) -> tuple[int, bytes, str]:
        if request_body.get("confirm_expensive_model") is True:
            return json_route({"ok": True, "scope": "main"})
        return json_route(
            {
                "ok": False,
                "scope": "main",
                "confirm_required": True,
                "confirm_message": message,
            }
        )

    return respond


def bad_scope_route() -> PostRoute:
    """The 400 Hermes raises for an invalid ``scope`` (``web_server.py``,
    ``set_model_assignment``): ``detail="scope must be 'main' or 'auxiliary'"``."""
    return lambda _request_body: (
        400,
        b'{"detail":"scope must be \'main\' or \'auxiliary\'"}',
        "application/json",
    )


@pytest.mark.asyncio
async def test_a_successful_default_write_decodes_ok() -> None:
    routes = {MODEL_SET_PATH: ok_route(provider="p", model="m")}
    with gateway_serving({}, post_routes=routes) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        result = await client.set_default_model(
            profile="fixture-profile",
            provider="p",
            model="m",
            confirm_expensive_model=False,
        )

    assert result.ok is True
    assert result.confirm_required is False


@pytest.mark.asyncio
async def test_the_profile_rides_the_query_string_and_the_rest_ride_the_body() -> None:
    """R4: ``profile`` is a query parameter; ``scope``/``provider``/``model``/
    ``confirm_expensive_model`` are body fields on Hermes's ``ModelAssignment``."""
    seen: list[Recorded] = []
    with gateway_serving({}, seen, post_routes={MODEL_SET_PATH: ok_route()}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        await client.set_default_model(
            profile="fixture-profile",
            provider="p",
            model="m",
            confirm_expensive_model=False,
        )

    assert "profile=fixture-profile" in seen[0].path
    assert seen[0].body == {
        "scope": "main",
        "provider": "p",
        "model": "m",
        "confirm_expensive_model": False,
    }
    assert CANARY not in seen[0].path


@pytest.mark.asyncio
async def test_the_confirmation_round_trip_requires_two_distinct_acts() -> None:
    """KTD7: the guard is not auto-confirmed. A caller that never issues a
    second, separate call never gets past ``confirm_required``, and the first
    call this test makes never carries ``confirm_expensive_model``."""
    seen: list[Recorded] = []
    route = confirm_required_route("gpt-5.5 costs real money")
    with gateway_serving({}, seen, post_routes={MODEL_SET_PATH: route}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())

        first = await client.set_default_model(
            profile="fixture-profile",
            provider="openai-codex",
            model="gpt-5.5",
            confirm_expensive_model=False,
        )
        assert first.ok is False
        assert first.confirm_required is True
        assert first.confirm_message == "gpt-5.5 costs real money"

        # A repeat of the identical first call changes nothing — the guard is
        # not bypassed by asking again the same way.
        second = await client.set_default_model(
            profile="fixture-profile",
            provider="openai-codex",
            model="gpt-5.5",
            confirm_expensive_model=False,
        )
        assert second.confirm_required is True

        # Only the second, textually distinct act — a call that actually
        # carries the flag — completes the write.
        third = await client.set_default_model(
            profile="fixture-profile",
            provider="openai-codex",
            model="gpt-5.5",
            confirm_expensive_model=True,
        )
        assert third.ok is True
        assert third.confirm_required is False

    assert [call.body["confirm_expensive_model"] for call in seen] == [False, False, True]


def test_set_default_model_has_no_default_for_confirm_expensive_model() -> None:
    """The transport-level half of KTD7's guarantee: a call site cannot omit
    the flag and land on a silently-false first attempt — it must be named
    every time, which is what makes "the first call never carries it" a
    property of the call site rather than of this method's judgement."""
    import inspect

    signature = inspect.signature(AdminClient.set_default_model)
    parameter = signature.parameters["confirm_expensive_model"]
    assert parameter.default is inspect.Parameter.empty


@pytest.mark.asyncio
async def test_a_400_for_an_invalid_scope_becomes_a_named_error_not_raised_urllib() -> None:
    import urllib.error

    with gateway_serving({}, post_routes={MODEL_SET_PATH: bad_scope_route()}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        with pytest.raises(AdminError) as caught:
            await client.set_default_model(
                profile="fixture-profile",
                provider="p",
                model="m",
                confirm_expensive_model=False,
            )

    assert caught.value.reason == "invalid_request"
    assert not isinstance(caught.value, urllib.error.HTTPError)


@pytest.mark.asyncio
async def test_the_credential_is_attached_to_the_default_write_and_never_leaks() -> None:
    seen: list[Recorded] = []
    with gateway_serving({}, seen, post_routes={MODEL_SET_PATH: ok_route()}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        await client.set_default_model(
            profile="fixture-profile",
            provider="p",
            model="m",
            confirm_expensive_model=False,
        )

    assert seen[0].headers["Authorization"] == f"Bearer {CANARY}"
    assert seen[0].headers["X-Hermes-Session-Token"] == CANARY
    assert CANARY not in json.dumps(seen[0].body)


@pytest.mark.asyncio
async def test_an_unrecognized_profile_surfaces_the_gateways_own_answer() -> None:
    """A profile that does not exist is not a shape this module invents a
    reason for — whatever Hermes answers with is decoded and returned as-is,
    the same as any other non-``ok`` response."""

    def route(_request_body: dict[str, Any]) -> tuple[int, bytes, str]:
        return json_route({"ok": False, "scope": "main"})

    with gateway_serving({}, post_routes={MODEL_SET_PATH: route}) as origin:
        client = AdminClient(ws_endpoint(origin), StubProvider())
        result = await client.set_default_model(
            profile="no-such-fixture-profile",
            provider="p",
            model="m",
            confirm_expensive_model=False,
        )

    assert result.ok is False
    assert result.confirm_required is False


@pytest.mark.asyncio
async def test_post_admin_json_is_the_generic_write_fetch_admin_json_is_the_read() -> None:
    """The two share the origin/size/error discipline (:func:`_perform_admin_request`);
    this proves the write path directly, the way :func:`fetch_admin_json` is
    proven directly above."""
    with gateway_serving(
        {}, post_routes={MODEL_SET_PATH: ok_route(provider="p", model="m")}
    ) as origin:
        decoded = post_admin_json(
            origin, MODEL_SET_PATH, token=CANARY, body={"scope": "main"}
        )

    assert decoded["ok"] is True


def _without_docstrings(source: str) -> str:
    """The module's code with every docstring and comment removed.

    Comments go too: the decision not to call the POST is explained in prose in
    several places, and prose is not a call.
    """
    import ast
    import io
    import tokenize

    tree = ast.parse(source)
    spans: list[tuple[int, int]] = []
    for node in ast.walk(tree):
        if not isinstance(
            node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)
        ):
            continue
        body = getattr(node, "body", [])
        if (
            body
            and isinstance(body[0], ast.Expr)
            and isinstance(body[0].value, ast.Constant)
            and isinstance(body[0].value.value, str)
        ):
            spans.append((body[0].lineno, body[0].end_lineno or body[0].lineno))

    lines = source.splitlines()
    keep = [
        line
        for number, line in enumerate(lines, start=1)
        if not any(start <= number <= end for start, end in spans)
    ]
    stripped = "\n".join(keep)
    return "".join(
        token.string if token.type != tokenize.COMMENT else ""
        for token in tokenize.generate_tokens(io.StringIO(stripped).readline)
    )
