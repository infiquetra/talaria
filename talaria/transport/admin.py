"""Hermes's admin HTTP API — the second *surface* on the transport's HTTP seam.

KTD1 (2026-08-06 model-picker plan): this is not a second transport. It extends
the seam ``talaria/transport/refresh.py`` established — :func:`dashboard_origin_for`
for the scheme mapping and :func:`require_fetchable_origin` for the loopback and
scheme refusals — because that module already turns a gateway WebSocket URL into
an HTTP origin and performs an authenticated GET against it. A parallel
implementation would be a second place for those rules to drift.

**The credential form, pinned to source (KTD2, R6).**

``talaria/transport/credentials.py`` records the WebSocket half: at Hermes
``7f4d15515`` the upgrade credential is read exclusively from ``ws.query_params``
and no header is inspected. The HTTP surface is a *different reader* and had to be
pinned separately before any code depended on it. At Hermes ``863e31318``,
``hermes_cli/web_server.py``'s ``_has_valid_session_token`` accepts exactly two
header forms and its docstring names their standing:

    session_header = request.headers.get(_SESSION_HEADER_NAME, "")   # X-Hermes-Session-Token
    ...
    auth = request.headers.get("authorization", "")
    expected = f"Bearer {_SESSION_TOKEN}"

    "The dedicated session header avoids collisions with reverse proxies that
     already use ``Authorization`` (for example Caddy ``basic_auth``). We still
     accept the legacy Bearer path for backward compatibility with older
     dashboard bundles."

The query-parameter form the WebSocket uses is **not** available here.
``_QUERY_TOKEN_API_PATHS`` in the same module is a frozenset of exactly one path,
``/api/files/download``, and ``_has_valid_query_token`` returns ``False`` for
anything else — so KTD2's stated contingency ("if the source shows a query
parameter is the supported HTTP form, change the decision") did not fire.
**KTD2 is confirmed, not revised.**

Verified against the running gateway on 2026-08-06 rather than read alone, since
line numbers in another project are perishable and the installed checkout is
shallow (one commit), so its history cannot date either header. ``GET
/api/model/options``: no credential → 401; ``Authorization: Bearer <token>`` →
200; ``X-Hermes-Session-Token: <token>`` → 200; ``?token=<token>`` → 401.

**Why both headers are sent.** ``_has_valid_session_token`` is an *or*, so either
alone suffices against that build. Bearer alone is the compatible choice against
an older gateway — Hermes documents it as the form older bundles use, and ADR-0001
makes Talaria a client of a gateway it did not launch, so version skew is real and
undatable from here. The dedicated header alone is the correct choice behind a
reverse proxy that consumes ``Authorization``, which is the collision Hermes names
in that docstring. Sending both is right in both directions; the cost is that the
credential now has two header names, which is why :data:`CREDENTIAL_HEADERS` is a
named constant that the leak tests iterate rather than two literals at the call
site.

**``/api/model/info`` needs no credential, and is sent one anyway.** It sits in
``hermes_cli/dashboard_auth/public_paths.py``'s ``PUBLIC_API_PATHS`` — confirmed
live, no credential → 200 — while ``/api/model/options`` is gated. The credential
is attached uniformly regardless, because the allowlist is Hermes's decision to
revisit and a Talaria that only authenticated the endpoints that currently need it
would break silently on the release that gates one more.

**``GET /api/profiles`` is read; ``POST /api/profiles/active`` is not written,
ever (KTD5, U4).** There is no constant for that path in this module and no
method that could reach it, and ``tests/transport/test_admin.py`` asserts the
absence rather than trusting it. The POST sets a sticky preference for
*subsequent* CLI invocations and, in Hermes's own words, "does not retarget the
already-running dashboard process" — so calling it would change a setting on a
machine while changing nothing about the session in front of the operator.
Switching profile in Talaria means dialling a different gateway instead.

**Hermes publishes no per-profile endpoint, and that is a measured fact.** The
row builder ``_profile_to_dict`` (``hermes_cli/web_server.py``) sends fourteen
keys and none of them is a URL, a host or a port; confirmed against the running
gateway on 2026-08-06, whose 37-row answer carried exactly ``description``,
``description_auto``, ``distribution_name``, ``distribution_source``,
``distribution_version``, ``gateway_running``, ``has_alias``, ``has_env``,
``is_default``, ``model``, ``name``, ``path``, ``provider`` and ``skill_count``.
``path`` is a filesystem directory, not an endpoint, and the profile's listening
port is not recorded in it either — a profile's ``gateway_state.json`` carries a
pid and no port. So the endpoint a profile is dialled at comes from **Talaria's
own configuration** (``[profiles.endpoints]`` in ``config.toml``), and a profile
with no configured endpoint is listed and marked not dialable rather than
guessed at. The rejected alternative was deriving a port from the profile's
directory: it would make Talaria depend on Hermes's on-disk layout, which
ADR-0001 and this repository's standing preference for transport interfaces over
Hermes internals both refuse.

**Nothing here logs, reprs or prints the credential.** :class:`AdminError`
messages name a status code, a path, or a host — never a URL that arrived from
configuration, and never a header value. This mirrors :class:`RefreshError`'s
rule for the same reason: an endpoint is exactly the string an operator pastes a
token into.
"""

from __future__ import annotations

import asyncio
import json
import urllib.error
import urllib.request
from typing import Any, Literal
from urllib.parse import urlencode, urljoin, urlsplit

from talaria.domain.models_catalog import (
    CatalogError,
    ModelSelection,
    ProfileDirectory,
    ProviderCatalog,
    decode_model_selection,
    decode_profile_directory,
    decode_provider_catalog,
)
from talaria.transport.credentials import Credential, CredentialError, CredentialProvider
from talaria.transport.refresh import (
    MAX_INDEX_BYTES,
    RefreshError,
    dashboard_origin_for,
    require_fetchable_origin,
)

__all__ = [
    "CREDENTIAL_HEADERS",
    "MAX_RESPONSE_BYTES",
    "MODEL_INFO_PATH",
    "MODEL_OPTIONS_PATH",
    "PROFILES_PATH",
    "AdminClient",
    "AdminError",
    "AdminFailure",
    "admin_origin_for",
    "fetch_admin_json",
]

#: Every header name the credential is written into. Named once so the tests
#: that assert the value never reaches a log, a repr or an exception can iterate
#: it, and so adding a third form cannot quietly escape that assertion.
CREDENTIAL_HEADERS: tuple[str, ...] = ("Authorization", "X-Hermes-Session-Token")

#: The same read cap ``refresh.py`` applies, imported rather than restated. A
#: wrong origin answering with an endless stream must not be read into memory
#: forever, and the two surfaces have no reason to disagree about how much is
#: too much.
MAX_RESPONSE_BYTES = MAX_INDEX_BYTES

MODEL_OPTIONS_PATH = "/api/model/options"
MODEL_INFO_PATH = "/api/model/info"

#: The endpoint directory (U4). Read-only, and the *only* profiles path this
#: module names — see the module docstring on why ``/api/profiles/active`` has
#: no constant here.
PROFILES_PATH = "/api/profiles"

#: What went wrong, as a value a caller can branch on without parsing prose.
#: R7 requires a picker to distinguish "could not fetch" from "nothing here",
#: and R8 needs the 404 branch nameable on its own, so these are part of the
#: interface rather than an implementation detail of the message text.
AdminFailure = Literal[
    "unauthorized",
    "absent_capability",
    "unreachable",
    "refused_origin",
    "oversized_response",
    "malformed_response",
    "credential_unavailable",
    "http_error",
]


class AdminError(Exception):
    """An admin call that did not produce a value, with a branchable reason.

    Carries no credential and no whole endpoint. ``reason`` is the
    machine-readable half and ``str(self)`` the operator-readable half; a caller
    that renders only the string still says something true, and one that
    branches on ``reason`` does not have to match on prose.
    """

    def __init__(self, reason: AdminFailure, message: str) -> None:
        super().__init__(message)
        self.reason: AdminFailure = reason

    def __repr__(self) -> str:
        return f"AdminError(reason={self.reason!r}, message={str(self)!r})"


def admin_origin_for(endpoint: str) -> str:
    """The admin API's HTTP origin for a gateway WebSocket endpoint.

    Thin by design. The derivation lives in :func:`dashboard_origin_for`, and
    this wrapper exists only to translate its :class:`RefreshError` into this
    module's vocabulary so a caller handles one exception type.

    The origin is derived from the gateway URL and from nothing else: there is
    deliberately no separate HTTP setting to configure. A second knob could be
    pointed at a second host, and the failure mode of that mistake is a
    credential minted for one gateway being sent to another.
    """
    try:
        return dashboard_origin_for(endpoint)
    except RefreshError as exc:
        raise AdminError("refused_origin", str(exc)) from exc


def _credential_headers(token: str) -> dict[str, str]:
    """Both accepted header forms, built from one value. See the module docstring."""
    return {"Authorization": f"Bearer {token}", "X-Hermes-Session-Token": token}


def _build_url(origin: str, path: str, params: dict[str, str] | None) -> str:
    """Join a path onto an origin, refusing anything that leaves that origin.

    ``urljoin`` is used for the join and then the result is *checked* rather
    than trusted, because ``urljoin`` treats an absolute-URL ``path`` as a
    replacement for the origin. Callers pass module constants today, but a
    future caller passing a path built from a gateway response must not be able
    to redirect the credential to a host of the response's choosing.
    """
    candidate = urljoin(origin, path)
    origin_parts, candidate_parts = urlsplit(origin), urlsplit(candidate)
    if (origin_parts.scheme, origin_parts.netloc) != (
        candidate_parts.scheme,
        candidate_parts.netloc,
    ):
        raise AdminError(
            "refused_origin",
            f"refusing to send a credential to a different origin for path {path!r}",
        )
    return f"{candidate}?{urlencode(params)}" if params else candidate


def fetch_admin_json(
    origin: str,
    path: str,
    *,
    token: str,
    params: dict[str, str] | None = None,
    timeout: float = 15.0,
) -> Any:
    """GET one admin endpoint and return its decoded JSON, or raise :class:`AdminError`.

    Synchronous, matching :func:`~talaria.transport.refresh.fetch_dashboard_index`;
    :class:`AdminClient` is the async caller and offloads this to a thread.

    Every failure mode the plan enumerates becomes a named ``reason`` here rather
    than an exception from ``urllib`` reaching the caller: 401 is
    ``unauthorized``, 404 is ``absent_capability`` (the gateway is too old to
    carry the endpoint — R8's absent-capability branch), a refused connection is
    ``unreachable``, a body that is not JSON is ``malformed_response``, and a
    body that hits the cap is ``oversized_response``.
    """
    try:
        require_fetchable_origin(origin)
    except RefreshError as exc:
        raise AdminError("refused_origin", str(exc)) from exc

    url = _build_url(origin, path, params)
    request = urllib.request.Request(
        url,
        headers={"Accept": "application/json", **_credential_headers(token)},
        method="GET",
    )

    try:
        # nosec B310 - require_fetchable_origin above allowlists the scheme to
        # http/https and _build_url forbids the join from leaving that origin,
        # which together are what B310 asks to be audited.
        with urllib.request.urlopen(request, timeout=timeout) as response:  # nosec B310
            # One byte past the cap, so a body sitting exactly at the limit is
            # accepted and one over it is detected. Reading exactly the cap
            # cannot tell a full read from a truncated one.
            raw = response.read(MAX_RESPONSE_BYTES + 1)
            charset = response.headers.get_content_charset() or "utf-8"
    except urllib.error.HTTPError as exc:
        # Read and discard: an unread HTTPError body holds the socket open.
        # The body is never surfaced — Hermes's 401 detail is fixed text, but a
        # 500 handler's detail can quote request state.
        try:
            exc.read(MAX_RESPONSE_BYTES)
        except OSError:
            pass
        raise _http_error(exc.code, path) from exc
    except OSError as exc:
        raise AdminError(
            "unreachable",
            f"no gateway answered at {_host_of(origin)} for {path} ({exc})",
        ) from exc

    if len(raw) > MAX_RESPONSE_BYTES:
        raise AdminError(
            "oversized_response",
            f"{path} answered with more than {MAX_RESPONSE_BYTES} bytes; refusing to read it",
        )

    try:
        return json.loads(raw.decode(charset, "replace"))
    except (json.JSONDecodeError, LookupError) as exc:
        raise AdminError(
            "malformed_response",
            f"{path} did not answer with JSON",
        ) from exc


def _http_error(code: int, path: str) -> AdminError:
    if code == 401:
        return AdminError(
            "unauthorized",
            f"the gateway refused Talaria's credential at {path}; "
            "the dashboard may have restarted — try `talaria refresh-credential`",
        )
    if code == 404:
        return AdminError(
            "absent_capability",
            f"this gateway does not serve {path}; it predates the admin model API",
        )
    return AdminError("http_error", f"the gateway answered HTTP {code} at {path}")


def _host_of(origin: str) -> str:
    """The host alone, for a message. Never the whole origin, never the query."""
    try:
        return urlsplit(origin).hostname or "an unnamed host"
    except ValueError:
        return "an unnamed host"


class AdminClient:
    """Read-only admin calls for one gateway endpoint.

    The credential is resolved through an injected
    :class:`~talaria.transport.credentials.CredentialProvider` and re-resolved on
    **every call**, never held on this object. That is KTD11's per-dial rule
    (2026-08-02 prototype plan) applied to the HTTP surface for the same reason
    it applies to the WebSocket: caching here would acquire a silent dependency
    on the credential being a fixed reusable value, and KTD6 additionally needs a
    profile switch to re-resolve against the endpoint it switched to.

    Nothing about the credential's *source* is decided here. The provider decides
    that, which is what keeps this module out of the way of U3's removal of the
    environment source — no code path below reads an environment variable.
    """

    def __init__(
        self,
        endpoint: str,
        provider: CredentialProvider,
        *,
        timeout: float = 15.0,
    ) -> None:
        #: Derived from the gateway endpoint at construction, so a bad endpoint
        #: fails once and loudly rather than on each call.
        self.origin = admin_origin_for(endpoint)
        self._provider = provider
        self._timeout = timeout

    def __repr__(self) -> str:
        """Name the origin and nothing else.

        The generated-style repr would render ``self._provider``, and a provider
        is the one object in the system that can be holding a live credential.
        This class cannot fix a leaky provider's own repr, but it can decline to
        invoke it — which is what keeps ``AdminClient`` out of every traceback
        that would otherwise carry a token through it.
        """
        return f"AdminClient(origin={self.origin!r})"

    async def _credential(self) -> Credential:
        try:
            return await self._provider.acquire()
        except CredentialError as exc:
            raise AdminError("credential_unavailable", str(exc)) from exc

    async def _get(self, path: str, params: dict[str, str] | None = None) -> Any:
        credential = await self._credential()
        # ``to_thread`` and not a bare call: this runs inside Textual's event
        # loop, and a blocking ``urlopen`` there freezes the interface for the
        # whole timeout — which is exactly what a gateway that has stopped
        # answering would produce.
        return await asyncio.to_thread(
            fetch_admin_json,
            self.origin,
            path,
            token=credential.value,
            params=params,
            timeout=self._timeout,
        )

    async def model_options(self, *, profile: str | None = None) -> ProviderCatalog:
        """``GET /api/model/options`` decoded into a :class:`ProviderCatalog`."""
        params = {"profile": profile} if profile else None
        try:
            return decode_provider_catalog(await self._get(MODEL_OPTIONS_PATH, params))
        except CatalogError as exc:
            raise AdminError("malformed_response", str(exc)) from exc

    async def model_info(self, *, profile: str | None = None) -> ModelSelection:
        """``GET /api/model/info`` decoded into a :class:`ModelSelection`."""
        params = {"profile": profile} if profile else None
        try:
            return decode_model_selection(await self._get(MODEL_INFO_PATH, params))
        except CatalogError as exc:
            raise AdminError("malformed_response", str(exc)) from exc

    async def list_profiles(self) -> ProfileDirectory:
        """``GET /api/profiles`` decoded into a :class:`ProfileDirectory` (U4).

        Named ``list_profiles`` rather than ``profiles`` so it cannot be
        mistaken for a property holding a cached directory. It is a live read
        every time, for the same reason :meth:`model_options` is: the answer
        includes ``gateway_running``, which is a fact about right now, and a
        cached "running" is exactly the claim that sends an operator into a
        dial against a gateway that has since stopped.

        There is no write counterpart on this class, deliberately (KTD5).
        """
        try:
            return decode_profile_directory(await self._get(PROFILES_PATH))
        except CatalogError as exc:
            raise AdminError("malformed_response", str(exc)) from exc
