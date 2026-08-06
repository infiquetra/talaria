"""Refresh the loopback credential file from a running Hermes dashboard.

**Why this exists.** The dashboard's session token is not a configured secret an
operator can look up. ``hermes_cli/web_server.py:300`` resolves it as
``HERMES_DASHBOARD_SESSION_TOKEN`` *or* a fresh ``secrets.token_urlsafe(32)``
minted at server start, holds it in memory only, and injects it into the SPA's
bootstrap script so the web UI can authenticate. It dies with the process. So
every dashboard restart invalidates ``<config_dir>/credentials``, and the only
place the new value exists is the page the dashboard already serves to any
client that can reach it.

That makes "read the token out of the page" the *supported* path rather than a
trick: it is precisely what the dashboard's own web UI does on every load.

**What this module refuses to do.**

* It will not fetch a credential from a non-loopback host over plain HTTP. A
  token read over cleartext from another machine is a token handed to whoever is
  on the path, and this module exists to make credential handling easier, which
  is exactly when a quiet downgrade does the most damage.
* It will not write the credential anywhere but a file created at ``0600``, and
  it writes through a temporary file in the same directory so the token is never
  readable through a window where the mode has not been applied yet.
* It never returns, logs, reprs or prints the token. :class:`RefreshReport`
  carries what happened, not what was written.

**The gated dashboard is a refusal, not a failure.** When Hermes runs its OAuth
gate the token is deliberately not injected at all — the SPA authenticates by
cookie and dials with a per-request ticket instead
(``hermes_cli/web_server.py:15835``). v0.1 implements only the loopback token
provider, so that case is reported as the unimplemented feature it is rather
than as a missing token.
"""

from __future__ import annotations

import json
import os
import re
import tempfile
import tomllib
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

__all__ = [
    "LOOPBACK_HOSTS",
    "RefreshError",
    "RefreshReport",
    "dashboard_origin_for",
    "extract_session_token",
    "fetch_dashboard_index",
    "refresh_credential",
    "require_fetchable_origin",
    "write_token",
]

#: Hosts whose traffic never leaves the machine, so plain HTTP carries nothing
#: an observer could not already read out of the process table.
LOOPBACK_HOSTS = frozenset({"127.0.0.1", "::1", "localhost"})

#: How much of the dashboard page to read. The bootstrap script is in the first
#: few kilobytes; the cap is here so a wrong URL answering with an endless
#: stream cannot be read into memory forever.
MAX_INDEX_BYTES = 2 * 1024 * 1024

_TOKEN_PATTERN = re.compile(r'__HERMES_SESSION_TOKEN__\s*=\s*"([^"]+)"')
_AUTH_REQUIRED_PATTERN = re.compile(r"__HERMES_AUTH_REQUIRED__\s*=\s*true")
_TOKEN_LINE = re.compile(r"^[ \t]*token[ \t]*=.*$", re.MULTILINE)

_WEB_SCHEME_FOR = {"ws": "http", "wss": "https", "http": "http", "https": "https"}


class RefreshError(Exception):
    """A refresh that did not happen, with a reason an operator can act on.

    Carries no credential and no whole endpoint: an endpoint is exactly the
    string an operator pastes a token into, so messages here name a scheme, a
    host, or a path, never a URL that arrived from configuration.
    """


@dataclass(frozen=True)
class RefreshReport:
    """What a refresh did. Deliberately says nothing about *what* it wrote."""

    path: Path
    origin: str
    created: bool
    tightened: bool
    preserved_keys: tuple[str, ...]


def dashboard_origin_for(endpoint: str) -> str:
    """The dashboard's HTTP origin for a gateway websocket endpoint.

    ``ws://127.0.0.1:9119/api/ws`` becomes ``http://127.0.0.1:9119/``. The path
    and query are dropped rather than rewritten, which is also what keeps a
    ``?token=`` on the input from surviving into the output — though callers are
    expected to pass a
    :attr:`~talaria.transport.attach.AttachTarget.url`, which has already been
    stripped.
    """
    parts = urlsplit(endpoint)
    scheme = _WEB_SCHEME_FOR.get(parts.scheme.lower())
    if scheme is None:
        raise RefreshError(
            f"cannot derive a dashboard address from a {parts.scheme or 'schemeless'} endpoint; "
            "pass --from with the dashboard's http URL"
        )
    if not parts.netloc:
        raise RefreshError(
            "the configured endpoint names no host; pass --from with the dashboard's http URL"
        )
    return urlunsplit((scheme, parts.netloc, "/", "", ""))


def require_fetchable_origin(origin: str) -> None:
    """Refuse anything but https, or http to a host on this machine.

    Two separate refusals share one function because they guard one call. The
    scheme allowlist keeps ``file:``, ``ftp:`` and the rest away from
    :func:`urllib.request.urlopen`, which will happily open them and would turn
    a wrong ``--from`` into an arbitrary-file read. The loopback rule then keeps
    a cleartext credential fetch on the machine it started on.

    **Public because ``talaria/transport/admin.py`` calls it too** (KTD1 of the
    2026-08-06 model-picker plan). The admin API is a second surface on the same
    HTTP seam, and both surfaces carry the same credential to the same origin —
    so a second copy of these two refusals would be a second place for them to
    drift, and the copy that drifted would be the one that stopped refusing.
    """
    parts = urlsplit(origin)
    scheme = parts.scheme.lower()
    if scheme not in {"http", "https"}:
        raise RefreshError(
            f"refusing to fetch a credential over {scheme or 'no'} scheme; "
            "the dashboard address must be http or https"
        )
    if scheme == "https":
        return
    try:
        host = (parts.hostname or "").lower()
    except ValueError as exc:  # a netloc urlsplit will parse but not interpret
        raise RefreshError(f"the dashboard address is not usable: {exc}") from exc
    if host in LOOPBACK_HOSTS:
        return
    raise RefreshError(
        f"refusing to read a credential from {host or 'an unnamed host'} over plain HTTP; "
        "use an https URL, or forward the dashboard to loopback over SSH"
    )


def fetch_dashboard_index(origin: str, *, timeout: float = 10.0) -> str:
    """GET the dashboard's index page, or raise :class:`RefreshError`."""
    require_fetchable_origin(origin)
    request = urllib.request.Request(origin, headers={"Accept": "text/html"})
    try:
        # nosec B310 - the scheme is allowlisted to http/https immediately above,
        # in require_fetchable_origin, which is what B310 asks to be audited.
        with urllib.request.urlopen(request, timeout=timeout) as response:  # nosec B310
            charset = response.headers.get_content_charset() or "utf-8"
            return str(response.read(MAX_INDEX_BYTES).decode(charset, "replace"))
    except urllib.error.HTTPError as exc:
        raise RefreshError(f"the dashboard answered HTTP {exc.code} at {origin}") from exc
    except OSError as exc:
        raise RefreshError(f"no dashboard answered at {origin} ({exc})") from exc


def extract_session_token(html: str) -> str:
    """Pull the injected session token out of the dashboard's bootstrap script."""
    match = _TOKEN_PATTERN.search(html)
    if match:
        return match.group(1)
    if _AUTH_REQUIRED_PATTERN.search(html):
        raise RefreshError(
            "this dashboard runs the OAuth gate, which issues a per-dial ticket instead "
            "of a session token, and Talaria v0.1 implements only the loopback token "
            "provider (GatedTicketProvider is specified in "
            "docs/engineering-journal/QUEUED.md and deliberately not built)"
        )
    raise RefreshError(
        "the dashboard page carried no session token; it may still be building its web "
        "assets, or be older than the token-injecting build"
    )


def _preserved_keys(existing: str) -> tuple[str, ...]:
    """Every top-level key in the current file except ``token``, best effort."""
    try:
        document = tomllib.loads(existing)
    except tomllib.TOMLDecodeError:
        # Malformed on the way in. The text is still preserved verbatim below;
        # what cannot be done is *report* what survived, and claiming a key
        # survived without having parsed it would be the wrong kind of guess.
        return ()
    return tuple(sorted(key for key in document if key != "token"))


def write_token(path: Path, token: str) -> tuple[bool, bool, tuple[str, ...]]:
    """Write ``token`` into ``path`` at ``0600``, preserving every other line.

    Returns ``(created, tightened, preserved_keys)``.

    The file is rewritten line-wise rather than round-tripped through a TOML
    serializer, so an operator's ``url`` key, their comments, and their spacing
    all survive a refresh. Round-tripping would silently reformat a file whose
    contents someone chose by hand.

    The write goes to a temporary file in the same directory and is then
    :func:`os.replace`\\ d over the target. :func:`tempfile.mkstemp` creates at
    ``0600``, so there is no instant at which the new credential exists at a
    looser mode — which a plain ``open`` on an already-``0644`` file would
    produce, because ``O_CREAT``'s mode is ignored when the file exists.
    """
    directory = path.parent
    directory.mkdir(parents=True, exist_ok=True)

    created = not path.exists()
    tightened = False
    preserved: tuple[str, ...] = ()

    line = f"token = {json.dumps(token)}"
    if created:
        content = f"{line}\n"
    else:
        tightened = bool(path.stat().st_mode & 0o177)
        existing = path.read_text(encoding="utf-8")
        preserved = _preserved_keys(existing)
        if _TOKEN_LINE.search(existing):
            content = _TOKEN_LINE.sub(lambda _: line, existing, count=1)
        else:
            separator = "" if existing.endswith("\n") or not existing else "\n"
            content = f"{existing}{separator}{line}\n"

    handle_fd, temporary = tempfile.mkstemp(dir=directory, prefix=".credentials-")
    try:
        with os.fdopen(handle_fd, "w", encoding="utf-8") as handle:
            handle.write(content)
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    except BaseException:
        # ``BaseException`` and not ``Exception``, deliberately: a Ctrl-C during
        # the write would otherwise leave a temporary file holding a live
        # credential behind in the config directory.
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise

    return created, tightened, preserved


def refresh_credential(
    origin: str, path: Path, *, timeout: float = 10.0
) -> RefreshReport:
    """Fetch the dashboard's session token and write it to ``path``."""
    token = extract_session_token(fetch_dashboard_index(origin, timeout=timeout))
    if not token.strip():
        raise RefreshError("the dashboard injected an empty session token")
    created, tightened, preserved = write_token(path, token)
    return RefreshReport(
        path=path,
        origin=origin,
        created=created,
        tightened=tightened,
        preserved_keys=preserved,
    )
