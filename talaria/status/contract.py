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
from typing import Any, Literal, cast
from urllib.parse import urlsplit, urlunsplit

from talaria.domain.projection import StatusPayload
from talaria.recorder.redact import is_suspicious_key
from talaria.text import defang

# ── KTD5's frozen limits ────────────────────────────────────────────────

#: Default tick interval, seconds. The operator's
#: ``status.interval_seconds`` config setting (``talaria/config.py``)
#: overrides it — this constant is the value used only when nothing else
#: supplies one (e.g. a caller that builds a runner directly in a test).
DEFAULT_INTERVAL_SECONDS: int = 5

#: The true-bottom bar's public segment names, in default display order.
StatusSegmentName = Literal[
    "cwd",
    "git_branch",
    "agent_model",
    "context",
    "task_progress",
    "connection",
    "version",
]
DEFAULT_STATUS_SEGMENTS: tuple[StatusSegmentName, ...] = (
    "cwd",
    "git_branch",
    "agent_model",
    "context",
    "task_progress",
    "connection",
    "version",
)
KNOWN_STATUS_SEGMENTS: frozenset[str] = frozenset(DEFAULT_STATUS_SEGMENTS)

DEFAULT_CWD_MAX_COLUMNS = 24
DEFAULT_GIT_BRANCH_MAX_COLUMNS = 18
DEFAULT_AGENT_MODEL_MAX_COLUMNS = 24

STATUS_INTERVAL_RANGE = (1, 3600)
CWD_MAX_COLUMNS_RANGE = (8, 48)
GIT_BRANCH_MAX_COLUMNS_RANGE = (8, 40)
AGENT_MODEL_MAX_COLUMNS_RANGE = (10, 48)

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

# ── The versioned script-output intake (#125) ─────────────────────────────
#
# A status script's stdout is either v1 plain-text rows (one row per line,
# rendered literally — the contract ``docs/formats/status-line.md`` freezes)
# or a versioned JSON document carried on that same stdout. Version 2 is the
# only document version this Talaria understands; it adds script-controlled
# rows (text plus a requested color) that render on the true-bottom bar
# rather than in the in-body status region.

#: The only script-output document version understood here. A document
#: naming any other version is an unknown shape, not an older dialect:
#: there never was a v1 JSON output shape, so ``{"version": 1, ...}`` on
#: stdout is rejected rather than guessed at.
SCRIPT_OUTPUT_VERSION: int = 2

#: Top-level keys a version-2 script document may carry. Exact, like
#: :data:`FROZEN_TOP_LEVEL_FIELDS`: an unknown key is a shape the reader
#: does not understand, and rendering around it is how junk reaches a bar.
SCRIPT_DOCUMENT_FIELDS: frozenset[str] = frozenset({"version", "rows"})

#: Keys one version-2 row object may carry. ``color`` is optional and
#: defaults to ``"text"``; anything else is an unknown shape.
SCRIPT_ROW_FIELDS: frozenset[str] = frozenset({"text", "color"})


@dataclass(frozen=True)
class ScriptRow:
    """One script-controlled bar row from a version-2 output document.

    ``color`` is the script's *request*, not a style: the bar maps it
    through its safe-color rules at render time (unknown names fall back
    to ``"text"``), so no free-form color value ever reaches the terminal
    framework. Carried raw here so the intake stays a shape check rather
    than growing a presentation mapping the domain core must not own.
    """

    text: str
    color: str = "text"


class ScriptDocumentError(ValueError):
    """A JSON-object stdout that is not a valid version-2 script document.

    Raised rather than rendered: an unknown shape on stdout is a broken
    script talking, and the bar keeps its previous good render with a
    notice instead of showing junk. A ``ValueError`` (not ``AssertionError``)
    because this fires at runtime on operator-script output, where a crash
    is never the answer — the runner converts it to a categorical tick
    outcome, the same way every other per-tick failure is reported.
    """


def parse_script_rows(text: str) -> tuple[ScriptRow, ...] | None:
    """Split version-2 script documents from v1 plain-text rows.

    Returns ``None`` when ``text`` is not a JSON object at all — that is
    the v1 plain-text path, which the caller renders byte-identically
    through the existing text normalizer. (Scalar and array JSON —
    ``123``, ``null``, ``[1]`` — also return ``None``: a v1 script
    printing a bare number keeps rendering that number rather than
    tripping a document protocol it never opted into. Only an object
    claims structure, so only an object is held to the document shape.)

    Returns the script rows (possibly empty: an explicit ``"rows": []``
    clears the bar to its segment row) for a valid version-2 document.

    Raises :class:`ScriptDocumentError` for a JSON object that is not a
    valid version-2 document: a missing or jumped ``version``, a
    missing or wrong-typed ``rows``, a null or wrong-typed row, or any
    unknown key at either level. The caller reports that loudly (a
    categorical marker, never a render of the junk) while the bar keeps
    its previous good rows.
    """
    try:
        document = json.loads(text)
    except (json.JSONDecodeError, UnicodeDecodeError):
        return None
    if not isinstance(document, dict):
        return None
    version = document.get("version", None)
    if version != SCRIPT_OUTPUT_VERSION:
        raise ScriptDocumentError(
            f"unknown script document version {version!r}; "
            f"this Talaria renders version {SCRIPT_OUTPUT_VERSION}"
        )
    unknown_top = set(document.keys()) - SCRIPT_DOCUMENT_FIELDS
    if unknown_top:
        raise ScriptDocumentError(
            f"unknown script document field(s): {sorted(unknown_top)}"
        )
    if not isinstance(document.get("rows"), list):
        raise ScriptDocumentError(
            'a version-2 script document needs "rows" as a list of rows'
        )
    rows: list[ScriptRow] = []
    for entry in document["rows"]:
        if isinstance(entry, str):
            rows.append(ScriptRow(text=entry))
            continue
        if not isinstance(entry, dict):
            raise ScriptDocumentError(
                f"a version-2 row is text or an object, not {type(entry).__name__}"
            )
        unknown_row = set(entry.keys()) - SCRIPT_ROW_FIELDS
        if unknown_row:
            raise ScriptDocumentError(
                f"unknown script row field(s): {sorted(unknown_row)}"
            )
        if not isinstance(entry.get("text"), str):
            raise ScriptDocumentError('a version-2 row object needs "text" as a string')
        color = entry.get("color", "text")
        if not isinstance(color, str):
            raise ScriptDocumentError('a version-2 row "color" must be a string')
        rows.append(ScriptRow(text=entry["text"], color=color))
    return tuple(rows)

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

#: Provider credential names that never forward, even when the operator names
#: them on ``environment.allowlist``. ``is_suspicious_key`` does not flag
#: these vendor-prefixed names — its API-key pattern anchors to the whole
#: name, so ``OPENAI_API_KEY`` reads as "some vendor's key" rather than a
#: credential shape — which is exactly why they are enumerated here instead
#: of relying on pattern coverage. Compared case-insensitively; the names
#: themselves are the only thing recorded, never any value.
DENIED_PROVIDER_KEYS: frozenset[str] = frozenset(
    {
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "AWS_ACCESS_KEY_ID",
        "GITHUB_PAT",
    }
)


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
    ``TALARIA_*`` set and the operator allowlist both. The exact provider
    names in :data:`DENIED_PROVIDER_KEYS` are denied first, by name, because
    the pattern check does not flag them — deny outranks the allowlist for
    those names unconditionally.

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
        if name.upper() in DENIED_PROVIDER_KEYS:
            return
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


def parse_command(command: object) -> tuple[tuple[str, ...] | None, str | None]:
    """Split ``status.command`` and name why an invalid value was disabled.

    ``config.py`` stores ``status.command`` as the plain command-line string an
    operator writes in ``config.toml`` (``command = "git status"``); KTD5
    requires the child be exec'd directly from an argv array, never through a
    shell, so the split happens here rather than at the config layer.
    ``shlex.split`` gives POSIX quoting semantics without ever invoking
    ``/bin/sh``. ``None`` means intentionally disabled, so it has no notice.
    An empty string, a non-string value, or a string with invalid POSIX quoting
    disables only the optional status command and returns a visible reason.

    The reason never repeats the configured command. A command may itself carry
    sensitive text, and the operator needs the bad key and fallback, not another
    copy of its contents in a terminal or log.
    """
    if command is None:
        return None, None
    if not isinstance(command, str):
        return None, "status.command must be a non-empty string; the status command is disabled"
    if not command.strip():
        return None, "status.command must not be empty; the status command is disabled"
    try:
        parts = shlex.split(command)
    except ValueError:
        return None, "status.command has invalid quoting; the status command is disabled"
    if not parts:
        return None, "status.command must not be empty; the status command is disabled"
    return tuple(parts), None


def normalize_bounded_integer(
    key: str,
    value: object,
    *,
    default: int,
    minimum: int,
    maximum: int,
) -> tuple[int, str | None]:
    """Return an integer setting within its inclusive range, plus any notice."""
    if isinstance(value, int) and not isinstance(value, bool) and minimum <= value <= maximum:
        return value, None
    return (
        default,
        f"{key} must be an integer from {minimum} through {maximum}; using {default}",
    )


def normalize_status_segments(
    value: object,
) -> tuple[tuple[StatusSegmentName, ...], tuple[str, ...]]:
    """Normalize configured bar rows while preserving the first known occurrence.

    A non-list value cannot express an order, so it receives the complete
    default. A list with no recognized row keeps the bar useful by showing the
    mandatory connection fallback alone. Unknown, non-string, and duplicate
    entries are skipped and reported without preventing recognized rows.
    """
    if not isinstance(value, (list, tuple)):
        return (
            DEFAULT_STATUS_SEGMENTS,
            ("status.segments must be a list of segment names; using the default order",),
        )

    segments: list[StatusSegmentName] = []
    notices: list[str] = []
    seen: set[str] = set()
    for entry in value:
        if not isinstance(entry, str) or entry not in KNOWN_STATUS_SEGMENTS:
            display = repr(defang(entry)) if isinstance(entry, str) else repr(entry)
            notices.append(f"status.segments contains unknown segment {display}; ignoring it")
            continue
        if entry in seen:
            notices.append(f"status.segments repeats {entry}; ignoring the duplicate")
            continue
        seen.add(entry)
        segments.append(cast(StatusSegmentName, entry))

    if not segments:
        notices.append("status.segments contains no known segment; showing connection only")
        segments.append("connection")
    return tuple(segments), tuple(notices)


@dataclass(frozen=True)
class StatusBarSettings:
    """The restart-resolved settings consumed by the bottom-bar renderer."""

    segments: tuple[StatusSegmentName, ...] = DEFAULT_STATUS_SEGMENTS
    cwd_max_columns: int = DEFAULT_CWD_MAX_COLUMNS
    git_branch_max_columns: int = DEFAULT_GIT_BRANCH_MAX_COLUMNS
    agent_model_max_columns: int = DEFAULT_AGENT_MODEL_MAX_COLUMNS

    def toggled(self, segment: str) -> tuple[StatusBarSettings, str | None]:
        """Toggle one known segment in memory, appending newly shown rows."""
        if segment not in KNOWN_STATUS_SEGMENTS:
            return self, f"bar: unknown segment {segment}"
        if segment in self.segments:
            segments = tuple(name for name in self.segments if name != segment)
        else:
            segments = (*self.segments, cast(StatusSegmentName, segment))
        return (
            StatusBarSettings(
                segments=segments,
                cwd_max_columns=self.cwd_max_columns,
                git_branch_max_columns=self.git_branch_max_columns,
                agent_model_max_columns=self.agent_model_max_columns,
            ),
            None,
        )


@dataclass(frozen=True)
class NormalizedStatusSettings:
    """Validated status values plus every operator-facing fallback notice."""

    interval_seconds: int
    bar: StatusBarSettings
    notices: tuple[str, ...] = ()


def normalize_status_settings(section: object) -> NormalizedStatusSettings:
    """Normalize the winning ``[status]`` table after config precedence resolves."""
    if not isinstance(section, Mapping):
        return NormalizedStatusSettings(
            interval_seconds=DEFAULT_INTERVAL_SECONDS,
            bar=StatusBarSettings(),
            notices=("status must be a table; using status defaults",),
        )

    segments, segment_notices = normalize_status_segments(
        section.get("segments", DEFAULT_STATUS_SEGMENTS)
    )
    interval, interval_notice = normalize_bounded_integer(
        "status.interval_seconds",
        section.get("interval_seconds", DEFAULT_INTERVAL_SECONDS),
        default=DEFAULT_INTERVAL_SECONDS,
        minimum=STATUS_INTERVAL_RANGE[0],
        maximum=STATUS_INTERVAL_RANGE[1],
    )
    cwd_max, cwd_notice = normalize_bounded_integer(
        "status.cwd_max_columns",
        section.get("cwd_max_columns", DEFAULT_CWD_MAX_COLUMNS),
        default=DEFAULT_CWD_MAX_COLUMNS,
        minimum=CWD_MAX_COLUMNS_RANGE[0],
        maximum=CWD_MAX_COLUMNS_RANGE[1],
    )
    branch_max, branch_notice = normalize_bounded_integer(
        "status.git_branch_max_columns",
        section.get("git_branch_max_columns", DEFAULT_GIT_BRANCH_MAX_COLUMNS),
        default=DEFAULT_GIT_BRANCH_MAX_COLUMNS,
        minimum=GIT_BRANCH_MAX_COLUMNS_RANGE[0],
        maximum=GIT_BRANCH_MAX_COLUMNS_RANGE[1],
    )
    agent_max, agent_notice = normalize_bounded_integer(
        "status.agent_model_max_columns",
        section.get("agent_model_max_columns", DEFAULT_AGENT_MODEL_MAX_COLUMNS),
        default=DEFAULT_AGENT_MODEL_MAX_COLUMNS,
        minimum=AGENT_MODEL_MAX_COLUMNS_RANGE[0],
        maximum=AGENT_MODEL_MAX_COLUMNS_RANGE[1],
    )
    notices = [*segment_notices]
    notices.extend(
        notice
        for notice in (interval_notice, cwd_notice, branch_notice, agent_notice)
        if notice is not None
    )
    return NormalizedStatusSettings(
        interval_seconds=interval,
        bar=StatusBarSettings(
            segments=segments,
            cwd_max_columns=cwd_max,
            git_branch_max_columns=branch_max,
            agent_model_max_columns=agent_max,
        ),
        notices=tuple(notices),
    )


@dataclass(frozen=True)
class ProcessLimits:
    """Bundles the numeric KTD5 limits so :mod:`runner` takes one argument."""

    timeout_seconds: float = TIMEOUT_SECONDS
    stdout_limit_bytes: int = STDOUT_LIMIT_BYTES
    stderr_limit_bytes: int = STDERR_LIMIT_BYTES
    row_limit: int = ROW_LIMIT
