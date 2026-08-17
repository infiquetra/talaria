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
from typing import Any
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
    "write_profile_token",
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

#: A table header line, e.g. ``[profiles.alpha]``. Used to find where a
#: profile's entry starts and — just as importantly — where it ends, so a
#: rewrite of one profile's token cannot reach into the next profile's table.
#:
#: ``MULTILINE`` is load-bearing, not stylistic: without it this pattern
#: anchors to the whole document and finds no header at all, so
#: :func:`_rewrite_top_level_token` would see no boundary and rewrite the first
#: ``token`` line anywhere in the file — which for a document whose first table
#: is ``[profiles.<name>]`` means a *default*-profile refresh silently
#: overwriting a named profile's credential.
#:
#: The trailing-comment and ``[[…]]`` branches are load-bearing for the same
#: reason, and were missed on the first draft: ``[profiles.alpha]  # staging``
#: is legal TOML, and a pattern that ends the line at ``]`` does not see it.
#: An unseen header is not a header that is skipped — it is a *boundary that is
#: not there*, so a rewrite runs straight past it into the next profile's
#: table.  Reviewed and reproduced both directions: a default refresh writing
#: its token into ``[profiles.alpha]``, and an ``alpha`` refresh writing into
#: ``[profiles.beta]``.  Group 1 is the bracket run, group 2 the dotted key.
_TABLE_HEADER_LINE = re.compile(
    r"^[ \t]*(\[\[?)([^\[\]]*)\]\]?[ \t]*(?:#[^\n]*)?$", re.MULTILINE
)

#: Where a table's span *ends*, asked as loosely as possible.
#:
#: Deliberately not :data:`_TABLE_HEADER_LINE`. Twice now a rewrite has run past
#: a header that pattern could not match — first a trailing comment, then a
#: quoted key carrying a bracket (``["we[ird"]``, legal TOML that the key
#: charclass above excludes) — and each time the consequence was the same:
#: a foreign table's credential silently replaced, and the profile that was
#: just "paired" left with no token at all.
#:
#: The lesson is that *recognising* a header and *bounding* on one are different
#: jobs with opposite failure directions. Bounding too eagerly costs an insert
#: placed one table early, which is visible and harmless. Bounding too late
#: destroys a credential. So this asks the loosest question that is still true
#: of every TOML table header: the line's first non-whitespace character is
#: ``[``. Nothing that is not a header opens a line that way in this file's
#: sanctioned schema.
_TABLE_BOUNDARY_LINE = re.compile(r"^[ \t]*\[", re.MULTILINE)

#: A bare ``token = …`` assignment, anchored to a whole line.
_TOKEN_ASSIGNMENT_LINE = re.compile(r"^[ \t]*token[ \t]*=")

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
    #: Which entry was written: ``""`` for the credential file's top-level
    #: ``token`` (the default profile), or the profile name for a
    #: ``[profiles.<name>]`` entry (KTD5).
    profile: str = ""


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


def _verify_only_target_changed(
    existing: str, content: str, target: tuple[str, ...], token: str
) -> None:
    """Refuse a rewrite that touched anything but ``target``.

    **Why this exists, and why it is not a fifth pattern.** Line-wise editing of
    TOML is the right call for this file — an operator's comments, ``url`` key
    and spacing must survive a refresh, and a serializer round-trip would
    reformat a document someone kept by hand. But four rounds of adversarial
    review found four different ways a line-wise scanner misreads structure: a
    header with a trailing comment, an array-of-tables header, a quoted key
    carrying a bracket, and a header-shaped line that is really the inside of a
    multi-line string. Each fix taught the scanner one more shape, and the next
    round found the shape after it. Every one of them failed the same way —
    silently, destroying a credential or leaving a stale one authoritative,
    with the command reporting success.

    So the scanner stops being the safety mechanism. It is now an optimisation
    for preserving formatting, and *this* is the guarantee: parse the document
    before and after, and require that the result equals the original with
    exactly one key set. Any misread — a false span start, a false end, string
    content mistaken for structure, an insert that lands inside an array —
    changes some other key or fails to change the target, and is refused before
    anything reaches the disk. A shape nobody has thought of yet fails the same
    way, which is the property a pattern can never have.

    A file that did not parse on the way in is exempt, because there is nothing
    to diff against. It is still *edited* — line-wise and unverified, with its
    non-target lines preserved, which is what this module has always done — so
    "exempt" here means "unguarded", not "untouched". The exposure is bounded by
    the reader rather than by this gate: an unparseable credential file is
    refused loud on every read
    (:func:`~talaria.transport.credentials._read_credential_file`), so it cannot
    be holding a working credential that an unverified edit could destroy.
    """
    try:
        before = tomllib.loads(existing)
    except tomllib.TOMLDecodeError:
        return

    try:
        after = tomllib.loads(content)
    except tomllib.TOMLDecodeError as exc:
        raise RefreshError(
            "refusing to write: the edit would leave the credential file unparseable "
            f"({exc}). Nothing was written; the file on disk is unchanged"
        ) from exc

    expected: dict[str, Any] = _deep_copy_document(before)
    cursor = expected
    for key in target[:-1]:
        nested = cursor.get(key)
        if not isinstance(nested, dict):
            nested = {}
            cursor[key] = nested
        cursor = nested
    cursor[target[-1]] = token

    if after != expected:
        name = ".".join(target)
        raise RefreshError(
            f"refusing to write: the edit changed more than {name}. This file's shape "
            "is not one the line-wise editor reads correctly — nothing was written, and "
            "the file on disk is unchanged"
        )


def _deep_copy_document(document: Any) -> Any:
    """A copy deep enough to set one nested key without mutating the original."""
    if isinstance(document, dict):
        return {key: _deep_copy_document(value) for key, value in document.items()}
    if isinstance(document, list):
        return [_deep_copy_document(value) for value in document]
    return document


def _preserved_keys(existing: str, *, profile: str = "") -> tuple[str, ...]:
    """What survives this write, best effort — top-level keys and sibling profiles.

    For a default-profile write that is every top-level key except ``token``.
    For a ``[profiles.<name>]`` write it is every top-level key (none of them is
    being replaced) plus every *other* profile's entry, named
    ``profiles.<other>`` so the report distinguishes the file's two levels.
    """
    try:
        document = tomllib.loads(existing)
    except tomllib.TOMLDecodeError:
        # Malformed on the way in. The text is still preserved verbatim below;
        # what cannot be done is *report* what survived, and claiming a key
        # survived without having parsed it would be the wrong kind of guess.
        return ()
    if not profile:
        return tuple(sorted(key for key in document if key != "token"))

    keys = [key for key in document if key != "profiles"]
    section = document.get("profiles")
    if isinstance(section, dict):
        keys.extend(f"profiles.{name}" for name in section if name != profile)
    return tuple(sorted(keys))


def _table_path(header_line: str) -> tuple[str, ...] | None:
    """The dotted key a table header names, e.g. ``("profiles", "alpha")``.

    Bare keys only. A quoted key (``["profiles"."a b"]``) answers ``None``,
    which routes the write to the append branch rather than to a rewrite of a
    line this function only half understands — the conservative direction,
    since appending adds a table and rewriting could destroy one.

    An array-of-tables header (``[[profiles.alpha]]``) answers ``None`` for the
    same conservative reason: it is a *boundary* every rewrite must respect, so
    the pattern matches it, but it is not a table this writer knows how to edit
    and a profile entry is never one.
    """
    match = _TABLE_HEADER_LINE.match(header_line)
    if match is None or match.group(1) == "[[":
        return None
    parts = [part.strip() for part in match.group(2).split(".")]
    if not parts or any(not re.fullmatch(r"[A-Za-z0-9_-]+", part) for part in parts):
        return None
    return tuple(parts)


def write_profile_token(
    path: Path, profile: str, token: str
) -> tuple[bool, bool, tuple[str, ...]]:
    """Write ``[profiles.<profile>].token`` into ``path`` at ``0600`` (KTD5).

    Returns ``(created, tightened, preserved_keys)``, matching
    :func:`write_token`, whose atomic-replace mechanics this shares — the
    credential must never exist at a looser mode for even an instant, and that
    property belongs to one function rather than to two that agree today.

    Editing is line-wise for the same reason :func:`write_token`'s is: the
    operator's ``url`` key, their comments, their other profiles and their
    spacing all survive, which a TOML round-trip would silently reformat.
    Three cases, in order: the profile's table exists and holds a ``token``
    line (replace that line, and only within that table's own span); the table
    exists without one (insert directly under the header); no table (append a
    new one at the end of the file, where nothing can be captured by it).
    """
    from talaria.transport.credentials import CredentialError, validate_profile_name

    try:
        validate_profile_name(profile)
    except CredentialError as exc:
        raise RefreshError(str(exc)) from exc

    line = f"token = {json.dumps(token)}"
    created = not path.exists()
    tightened = False
    preserved: tuple[str, ...] = ()

    if created:
        content = f"[profiles.{profile}]\n{line}\n"
    else:
        tightened = bool(path.stat().st_mode & 0o177)
        existing = path.read_text(encoding="utf-8")
        preserved = _preserved_keys(existing, profile=profile)
        content = _rewrite_profile_token(existing, profile, line)
        _verify_only_target_changed(
            existing, content, ("profiles", profile, "token"), token
        )

    _atomic_write(path, content)
    return created, tightened, preserved


def _rewrite_profile_token(existing: str, profile: str, line: str) -> str:
    """Return ``existing`` with ``[profiles.<profile>]``'s token set to ``line``."""
    lines = existing.splitlines()
    wanted = ("profiles", profile)

    start: int | None = None
    end = len(lines)
    for index, text in enumerate(lines):
        # Two different questions, and conflating them was a real defect. "Is
        # this line a table header?" decides where a span ENDS, and every
        # header ends one — including the ones this writer refuses to edit.
        # "Which table does it name?" decides where the wanted span BEGINS, and
        # answers ``None`` for an array-of-tables or a quoted key. Skipping the
        # line whenever the second question answered ``None`` made those
        # headers invisible as boundaries, so an insert ran past ``[[servers]]``
        # and replaced a foreign table's token — the same
        # rewrite-crosses-a-boundary class the trailing-comment fix closed,
        # reintroduced one function over.
        if _TABLE_BOUNDARY_LINE.match(text) is None:
            continue
        table = _table_path(text)
        if start is None and table == wanted:
            start = index
            end = len(lines)
            continue
        if start is not None and index > start:
            end = index
            break

    if start is None:
        separator = "" if not existing or existing.endswith("\n") else "\n"
        blank = "" if not existing.strip() else "\n"
        return f"{existing}{separator}{blank}[profiles.{profile}]\n{line}\n"

    for index in range(start + 1, end):
        if _TOKEN_ASSIGNMENT_LINE.match(lines[index]):
            lines[index] = line
            break
    else:
        lines.insert(start + 1, line)
    return "\n".join(lines) + "\n"


def _atomic_write(path: Path, content: str) -> None:
    """Replace ``path`` with ``content`` through a same-directory 0600 temporary.

    :func:`tempfile.mkstemp` creates at ``0600``, so there is no instant at
    which the new credential exists at a looser mode — which a plain ``open``
    on an already-``0644`` file would produce, because ``O_CREAT``'s mode is
    ignored when the file exists.
    """
    directory = path.parent
    directory.mkdir(parents=True, exist_ok=True)

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


def write_token(path: Path, token: str) -> tuple[bool, bool, tuple[str, ...]]:
    """Write ``token`` into ``path`` at ``0600``, preserving every other line.

    Returns ``(created, tightened, preserved_keys)``.

    The file is rewritten line-wise rather than round-tripped through a TOML
    serializer, so an operator's ``url`` key, their comments, and their spacing
    all survive a refresh. Round-tripping would silently reformat a file whose
    contents someone chose by hand.

    The write goes to a temporary file in the same directory and is then
    :func:`os.replace`\\ d over the target — see :func:`_atomic_write`, which
    :func:`write_profile_token` shares so the mode discipline has one home.

    **The top-level ``token`` line is rewritten, and only that line.** A
    ``token`` under a ``[profiles.<name>]`` header belongs to that profile, and
    a regex that matched it would overwrite one profile's credential while
    reporting a default-profile refresh — so the search stops at the first
    table header.
    """
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
        content = _rewrite_top_level_token(existing, line)
        _verify_only_target_changed(existing, content, ("token",), token)

    _atomic_write(path, content)
    return created, tightened, preserved


def _rewrite_top_level_token(existing: str, line: str) -> str:
    """Set the document's top-level ``token``, never one inside a table."""
    header = _TABLE_BOUNDARY_LINE.search(existing) if "[" in existing else None
    boundary = header.start() if header is not None else len(existing)
    head, tail = existing[:boundary], existing[boundary:]

    if _TOKEN_LINE.search(head):
        return _TOKEN_LINE.sub(lambda _: line, head, count=1) + tail

    separator = "" if head.endswith("\n") or not head else "\n"
    return f"{head}{separator}{line}\n{tail}"


def refresh_credential(
    origin: str, path: Path, *, timeout: float = 10.0, profile: str = ""
) -> RefreshReport:
    """Fetch the dashboard's session token and write it to ``path``.

    ``profile`` selects which entry is written (KTD5): empty writes the
    top-level ``token`` — the default profile, and the whole of v0.1–v0.3
    behaviour — while a name writes ``[profiles.<name>].token``. Each profile's
    dashboard mints its own token, so pairing a second profile means running
    this once per profile against *that* profile's dashboard origin.
    """
    token = extract_session_token(fetch_dashboard_index(origin, timeout=timeout))
    if not token.strip():
        raise RefreshError("the dashboard injected an empty session token")
    if profile:
        created, tightened, preserved = write_profile_token(path, profile, token)
    else:
        created, tightened, preserved = write_token(path, token)
    return RefreshReport(
        path=path,
        origin=origin,
        created=created,
        tightened=tightened,
        preserved_keys=preserved,
        profile=profile,
    )
