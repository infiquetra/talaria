"""KTD5's frozen status-child contract: limits, the environment deny boundary,
and the payload the child receives on stdin.

Nothing here spawns a process — that is :mod:`talaria.status.runner`. This
module is deliberately import-only-of-data-and-pure-functions so
``tests/status/test_env.py`` and ``tests/status/test_payload_schema.py`` can
exercise the contract in isolation from asyncio and subprocess machinery.

The child-environment deny boundary imports
:func:`talaria.recorder.redact.is_suspicious_key` rather than re-deriving the
credential-shaped-name patterns (U2's :data:`~talaria.recorder.redact.SENSITIVE_KEY_PATTERNS`).
KTD5 is explicit that this is one copy of the boundary, not two, so the two
copies cannot drift apart.
"""

from __future__ import annotations

import json
import shlex
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any
from urllib.parse import urlsplit, urlunsplit

from talaria.domain.projection import StatusPayload
from talaria.recorder.redact import is_suspicious_key

# ── KTD5's frozen limits ────────────────────────────────────────────────

#: Default tick interval, seconds. KTD5's documented default; the operator's
#: ``status.interval_seconds`` config setting (``talaria/config.py``)
#: overrides it — this constant is the value used only when nothing else
#: supplies one (e.g. a caller that builds a runner directly in a test).
DEFAULT_INTERVAL_SECONDS: float = 10.0

#: KTD5: "timeout 2s then kill".
TIMEOUT_SECONDS: float = 2.0

#: KTD5: "output limit 16 KiB".
STDOUT_LIMIT_BYTES: int = 16 * 1024

#: KTD5: "stderr captured separately from stdout, capped at 4 KiB".
STDERR_LIMIT_BYTES: int = 4 * 1024

#: KTD5: "row bound 8 rows with visible truncation".
ROW_LIMIT: int = 8

#: The visible marker appended when stdout carried more rows than
#: :data:`ROW_LIMIT`, or more bytes than :data:`STDOUT_LIMIT_BYTES`.
TRUNCATION_MARKER: str = "… truncated"

# ── KTD5's environment deny boundary ────────────────────────────────────

#: Passed through unconditionally by name (not by prefix, so nothing here
#: collides with the TALARIA_* credential namespace check below).
_BASE_PASSTHROUGH_NAMES: frozenset[str] = frozenset({"PATH", "HOME", "SHELL", "TERM", "TMPDIR"})

#: ``LANG`` plus every ``LC_*`` locale variable is passed through by prefix.
#: ``LANG`` is forwarded by *exact* name and ``LC_`` by prefix. The distinction
#: is a real leak, not tidiness: matching ``LANG`` as a prefix forwards every
#: variable merely starting with those four letters, which in an ordinary
#: developer environment means ``LANGCHAIN_API_KEY`` and ``LANGSMITH_*`` go to
#: the status child by default. The credential-name deny list does not save it
#: either — ``is_suspicious_key`` anchors its API-key pattern to the whole name,
#: so ``LANGCHAIN_API_KEY`` is not suspicious to it.
_LOCALE_EXACT_NAMES: tuple[str, ...] = ("LANG",)
_LOCALE_PREFIXES: tuple[str, ...] = ("LC_",)

#: The exact five ``TALARIA_*`` variables KTD5 forwards. Any other
#: ``TALARIA_*`` name is dropped even if it also appears on the operator
#: allowlist — the credential-shaped-name deny below outranks both, but this
#: enumeration is the first gate: nothing outside this set is even
#: considered for forwarding under the ``TALARIA_`` prefix.
FORWARDED_TALARIA_VARS: tuple[str, ...] = (
    "TALARIA_CONFIG_DIR",
    "TALARIA_GATEWAY_URL",
    "TALARIA_PROFILE",
    "TALARIA_LOG_LEVEL",
    "TALARIA_STATUS_INTERVAL",
)

#: The one member of :data:`FORWARDED_TALARIA_VARS` whose value carries
#: potential credential material in its query string (KTD11's attach token
#: rides this URL). Its query string is stripped entirely rather than
#: pattern-filtered — see :func:`_strip_query`.
GATEWAY_URL_ENV_VAR = "TALARIA_GATEWAY_URL"


def _strip_query(url: str) -> str:
    """Drop the query string and fragment, keeping scheme/host/path only.

    KTD5: "the child has no use for query state, so dropping all of it costs
    nothing and does not depend on pattern coverage being complete." A
    pattern-based redaction is deliberately not used here — see KTD5's own
    rationale about ``ticket``/``internal`` slipping past ``redactUrl``.

    Userinfo is dropped with it. A URL is allowed to carry ``user:password@``
    ahead of the host, and that is credential material by definition, so
    rebuilding the URL from an untouched ``netloc`` would forward a password to
    the status child while carefully removing the query it rode in next to.
    """
    parts = urlsplit(url)
    # hostname/port rather than netloc: netloc still carries any userinfo.
    host = parts.hostname or ""
    if ":" in host:  # IPv6 literals need their brackets back.
        host = f"[{host}]"
    netloc = f"{host}:{parts.port}" if parts.port is not None else host
    return urlunsplit((parts.scheme, netloc, parts.path, "", ""))


def _sanitize(name: str, value: str) -> str:
    """Clean a value that is safe to forward by name but not as written.

    Applied to every variable on its way into the child environment, whichever
    rule selected it. Keyed by name because that is what makes it total: a
    sanitizer attached to one forwarding rule protects only the names that
    arrive through that rule, and the operator allowlist can name any variable
    at all — including one the enumerated ``TALARIA_*`` rule had already cleaned.
    """
    if name == GATEWAY_URL_ENV_VAR:
        return _strip_query(value)
    return value


def build_child_env(
    *,
    parent_env: Mapping[str, str],
    allowlist: Sequence[str] = (),
) -> dict[str, str]:
    """Build the status child's environment under KTD5's default-deny rule.

    ``parent_env`` is Talaria's own process environment (normally
    ``os.environ``, injected here so tests never touch the real one).
    ``allowlist`` is the operator-configured set of additional variable names
    to forward (``config.get("environment", "allowlist")``).

    Every candidate name — base, locale, ``TALARIA_*``, and allowlist alike —
    is asserted against :func:`~talaria.recorder.redact.is_suspicious_key`
    before being forwarded. A ``TALARIA_*`` prefix is not by itself a pass
    (KTD5): the credential-shaped-name deny outranks the enumerated
    ``TALARIA_*`` set and the operator allowlist both.

    Value sanitizing happens in :func:`_sanitize`, on the one path every
    forwarded variable takes, rather than in the loop that happens to know about
    a given name. It used to sit inside the ``TALARIA_*`` loop, which meant the
    operator allowlist — running afterwards, over the same names, with no
    sanitizing of its own — overwrote the cleaned ``TALARIA_GATEWAY_URL`` with
    the raw one and handed the attach token to the child. A rule that only holds
    when a name arrives through the expected loop is not a boundary.
    """
    child_env: dict[str, str] = {}

    def _maybe_forward(name: str, value: str) -> None:
        if is_suspicious_key(name):
            return
        child_env[name] = _sanitize(name, value)

    for name in _BASE_PASSTHROUGH_NAMES:
        if name in parent_env:
            _maybe_forward(name, parent_env[name])

    for name, value in parent_env.items():
        if name in _LOCALE_EXACT_NAMES or name.startswith(_LOCALE_PREFIXES):
            _maybe_forward(name, value)

    for name in FORWARDED_TALARIA_VARS:
        if name in parent_env:
            _maybe_forward(name, parent_env[name])

    for name in allowlist:
        if name in parent_env:
            _maybe_forward(name, parent_env[name])

    return child_env


def encode_payload(payload: StatusPayload) -> bytes:
    """Serialize KTD5's frozen status document for the child's stdin.

    ``StatusPayload.to_json_dict()`` (U3) is the single source of the field
    set; this function only encodes it — it does not itself decide field
    names or shapes, so ``docs/formats/status-line.md`` and this contract can
    both be asserted against ``StatusPayload`` rather than against each
    other.
    """
    return json.dumps(payload.to_json_dict()).encode("utf-8") + b"\n"


#: The frozen v1 field set, for the doc/serializer agreement test
#: (U6's "Verification" clause: "a test asserts the doc's field list matches
#: the serializer"). Top-level keys only, matching ``to_json_dict()``.
FROZEN_TOP_LEVEL_FIELDS: frozenset[str] = frozenset(
    {"version", "mode", "connection", "session", "turn", "pending_prompts", "subagents", "usage"}
)


def assert_frozen_shape(document: Mapping[str, Any]) -> None:
    """Raise ``AssertionError`` if ``document`` is not exactly KTD5's v1 shape.

    Used by the schema test (R19/R20) rather than duplicated field-by-field
    assertions in the test module.
    """
    actual = set(document.keys())
    if actual != FROZEN_TOP_LEVEL_FIELDS:
        missing = FROZEN_TOP_LEVEL_FIELDS - actual
        extra = actual - FROZEN_TOP_LEVEL_FIELDS
        raise AssertionError(f"status payload field drift: missing={missing} extra={extra}")
    if document["version"] != 1:
        raise AssertionError(f"status payload version drift: {document['version']!r} != 1")


def parse_command(command: str | None) -> tuple[str, ...] | None:
    """Split ``status.command`` (KTD15) into an argv array. No shell involved.

    ``config.py`` stores ``status.command`` as the plain command-line string an
    operator writes in ``config.toml`` (``command = "git status"``); KTD5
    requires the child be exec'd directly from an argv array, never through a
    shell, so the split happens here rather than at the config layer.
    ``shlex.split`` gives POSIX quoting semantics without ever invoking
    ``/bin/sh``. ``None`` or an all-whitespace string means "disabled" — no
    child is ever spawned, matching KTD5's "an absent ``status.command``
    disables the runner."
    """
    if command is None:
        return None
    parts = shlex.split(command)
    return tuple(parts) if parts else None


@dataclass(frozen=True)
class ProcessLimits:
    """Bundles the numeric KTD5 limits so :mod:`runner` takes one argument."""

    timeout_seconds: float = TIMEOUT_SECONDS
    stdout_limit_bytes: int = STDOUT_LIMIT_BYTES
    stderr_limit_bytes: int = STDERR_LIMIT_BYTES
    row_limit: int = ROW_LIMIT
