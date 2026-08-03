"""Redaction boundary for recorded gateway traffic.

Ported from ``src/record/redact.ts`` (the TypeScript reference), which is
itself pinned to Hermes ``7f4d15515`` (2026-08-01). Every frame passes through
here before it reaches disk. This is deliberately the first module written in
the Python recorder: the Hermes gateway carries credentials in plaintext on
ordinary client-to-server frames, and a recording that captures them cannot be
cleaned up afterwards once it is hash-chained.

Re-verified against ``tui_gateway/methods_prompt.py`` at the same pin during
this port (R27): the four ``@method(...)`` registrations below —
``clarify.respond``, ``terminal.read.respond``, ``sudo.respond``,
``secret.respond`` — are exactly the deny-set the TypeScript reference
encodes, confirmed by ``git -C ~/.hermes/hermes-agent show
7f4d15515:tui_gateway/methods_prompt.py``. A fifth credential-bearing method,
``model.save_key`` (``tui_gateway/methods_complete.py:350``), carries its
value under ``params.api_key`` — already covered by the key-name net below
(the ``...[-_]?keys?$`` pattern), so it needs no deny-set entry of its own;
U2's equivalence test proves it is caught anyway.

Hermes calls the four deny-set methods its "blocking bridges", and its own
protocol test groups all four under ``sensitive_prompt``:

    clarify.respond        -> params.answer
    terminal.read.respond  -> params.text
    sudo.respond           -> params.password
    secret.respond         -> params.value

The request side is safe -- ``sudo.request`` is emitted with an empty payload
-- so the exposure is entirely in the direction Talaria writes.

Redactions are recorded rather than silently applied, so a reader of the
corpus sees that something was withheld instead of a clean-looking hole.

**Deliberate divergence from the TypeScript reference (KTD6, R28, PC10).**
``redactUrl``'s ``SENSITIVE_KEY_PATTERNS`` withhold the bare ``token`` query
key but match neither ``ticket`` nor ``internal`` -- the other two credential
forms the Hermes attach protocol accepts (KTD11). Porting the pattern set
"exactly" and satisfying PC10 ("no URL-borne ticket or internal value
survives in the frame log") were mutually exclusive as written, and the
security property wins: :func:`redact_url` denies ``token``, ``ticket``, and
``internal`` by name, in addition to the shared :data:`SENSITIVE_KEY_PATTERNS`
net. This makes the Python redactor a strict superset of the TypeScript one.
The equivalence harness (``tests/recorder/test_equivalence.py``) asserts the
relation explicitly: every TypeScript ``redactions`` entry appears in the
Python output, and every additional Python entry is drawn from this
enumerated set (``ticket``, ``internal``) -- so the divergence stays pinned by
a test rather than drifting.

Two further divergences were added on 2026-08-03 after an adversarial probe put
credentials on disk through this boundary. Both are enumerated here for the
same reason as the first: an unlisted divergence is what KTD6 forbids.

1. *Key names are matched in a squashed form as well as a camel-normalized one.*
   ``_normalize_key`` alone let ``ApIkEy`` and ``api key`` through -- the first
   because inserting camel-boundary separators rewrites the name into
   ``ap_ik_ey``, the second because a space is not in the ``[-_]`` class the
   patterns anchor against. See :func:`_squashed_key`.

2. *String values are checked for credential-bearing URLs, not only key names.*
   A frame carrying ``{"url": "ws://host/api/ws?token=..."}`` wrote the token
   verbatim, because no key in that frame is suspicious. KTD11 places the attach
   credential in exactly that position. See :func:`_redact_credential_url`,
   which fires only when the string parses as an absolute URL whose query
   actually carries a denied parameter, so the corpus keeps its harmless URLs.

Both make the Python redactor a wider superset; neither withholds anything the
TypeScript reference withholds differently, and the usage counters this module
exists to preserve (``max_tokens`` and its siblings) are unaffected -- pinned by
the over-redaction controls in ``tests/recorder/test_redact.py``.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

#: A value withheld from the recording. The original never reaches disk.
REDACTED = "[redacted]"

#: Explicit deny-set, keyed by JSON-RPC method name.
#:
#: Each entry lists the ``params`` keys whose values must never be written.
#: Derived by reading the gateway's ``_respond`` dispatch rather than by
#: guessing at names (R27).
_DENY_BY_METHOD: dict[str, tuple[str, ...]] = {
    "sudo.respond": ("password",),
    "secret.respond": ("value",),
    # The serialized terminal buffer is arbitrary captured screen content and
    # can contain anything the operator's terminal was displaying, including
    # output from an unrelated program.
    "terminal.read.respond": ("text",),
    # Free-text the operator typed in reply to an agent's question. Not a
    # credential field by design, which is exactly why it needs an explicit
    # rule: the key name `answer` looks innocuous, so the net below never
    # catches it, and "paste the token here" is an ordinary thing for an
    # agent to ask.
    "clarify.respond": ("answer",),
}

#: Defence in depth against protocol drift.
#:
#: The deny-set above is exact and will go stale the first time Hermes adds a
#: credential-bearing method. Any parameter whose *name* looks like a secret
#: is withheld regardless of which method carried it, so a new method leaks
#: nothing before anyone notices it exists.
#:
#: These patterns anchor rather than substring-match, which matters more than
#: it looks. Hermes has seventeen distinct key names containing "token" and
#: exactly one of them is a credential -- the bare ``token``. The other
#: sixteen are usage counts (``input_tokens``, ``max_tokens``,
#: ``session_total_tokens``, ``tokens_per_delta``). A substring match on
#: "token" withholds all seventeen, which destroys the accounting fields the
#: corpus exists to study and makes every affected frame look tampered with.
#: Over-redaction is not the safe direction here; it is a different failure.
SENSITIVE_KEY_PATTERNS: tuple[re.Pattern[str], ...] = (
    re.compile(r"^(.*[-_])?pass(word|phrase)$", re.IGNORECASE),
    re.compile(r"secret", re.IGNORECASE),
    re.compile(r"^(.*[-_])?credentials?$", re.IGNORECASE),
    re.compile(r"^authorization$", re.IGNORECASE),
    re.compile(r"^(api|private|public|access|secret|signing)[-_]?keys?$", re.IGNORECASE),
    # Singular only. `access_token` is a credential; `max_tokens` is an integer.
    re.compile(r"^(.*[-_])?token$", re.IGNORECASE),
)

#: Query-parameter names withheld from a recorded ``endpoint``/attach URL in
#: addition to the key-name net above. ``token`` is already caught by
#: :data:`SENSITIVE_KEY_PATTERNS`; ``ticket`` and ``internal`` are the
#: Python-only superset (see module docstring, KTD6/PC10).
URL_ONLY_DENIED_QUERY_KEYS: frozenset[str] = frozenset({"ticket", "internal"})

_CAMEL_BOUNDARY = re.compile(r"([a-z0-9])([A-Z])")


def _normalize_key(key: str) -> str:
    """Normalize a key to snake_case so one set of patterns covers both conventions.

    The gateway is Python and sends ``access_token``; the TypeScript client
    sent ``authToken``. Anchored patterns need a separator to anchor against,
    so treat a lower-to-upper boundary as one.
    """
    return _CAMEL_BOUNDARY.sub(r"\1_\2", key).lower()


#: Anything that is plausibly a word separator in a key name. ``_normalize_key``
#: only ever produced ``_``, so ``api key`` and ``api.key`` did not match the
#: ``[-_]`` separator class the patterns anchor against.
_SEPARATORS = re.compile(r"[^0-9a-z]+")


def _squashed_key(key: str) -> str:
    """The key lowercased with every separator removed: ``ApI kEy`` -> ``apikey``.

    Tested in addition to :func:`_normalize_key`, not instead of it, because the
    two catch different things and neither is a superset. The camel normalizer
    turns ``accessToken`` into ``access_token``, which the anchored patterns
    need — but it also rewrites an oddly-cased ``ApIkEy`` into ``ap_ik_ey``,
    inserting separators mid-word and destroying the very name it is meant to
    canonicalize. The squashed form has no separators to be confused by.

    Squashing is conservative for the ``token`` pattern rather than
    over-eager: ``max_tokens`` squashes to ``maxtokens``, which still does not
    match a pattern anchored on ending in the singular ``token``. The usage
    counters this module deliberately preserves stay preserved.
    """
    return _SEPARATORS.sub("", key.lower())


def is_suspicious_key(key: str) -> bool:
    """True when a key name alone is reason enough to withhold its value."""
    candidates = (_normalize_key(key), _squashed_key(key))
    return any(
        pattern.search(candidate) for candidate in candidates for pattern in SENSITIVE_KEY_PATTERNS
    )


def redact_url(url: str) -> str:
    """Strip credentials from a URL's query string so it is safe to record."""
    parts = urlsplit(url)
    if not parts.scheme or not parts.netloc:
        # Not a parseable absolute URL. Withhold it rather than record an
        # unknown string that may itself be a credential.
        return REDACTED

    pairs = parse_qsl(parts.query, keep_blank_values=True)
    changed = False
    redacted_pairs: list[tuple[str, str]] = []
    for key, value in pairs:
        if is_suspicious_key(key) or key in URL_ONLY_DENIED_QUERY_KEYS:
            redacted_pairs.append((key, REDACTED))
            changed = True
        else:
            redacted_pairs.append((key, value))

    if not changed:
        return url

    # No `safe` override: JS's `URLSearchParams.toString()` percent-encodes
    # `[` and `]` too (`docs/formats/frame-log.md`'s own header example is
    # `token=%5Bredacted%5D`), so the default `quote_via=quote_plus` matches
    # the reference byte-for-byte on the parts that matter for KTD6 (an
    # exact-value comparison is only made for *un*-redacted query values;
    # redacted ones are compared by presence/state only).
    new_query = urlencode(redacted_pairs)
    return urlunsplit((parts.scheme, parts.netloc, parts.path, new_query, parts.fragment))


@dataclass(frozen=True)
class Redaction:
    """One withheld value, reported alongside the frame it was removed from."""

    #: Dotted path to the removed value, e.g. ``params.password``.
    path: str
    #: Why it was removed -- either the explicit rule or the key-name net.
    reason: str


@dataclass(frozen=True)
class RedactResult:
    """The result of passing one frame through the boundary."""

    #: A copy safe to write. The input is never mutated.
    frame: Any
    #: What was withheld, in the order encountered. Empty when nothing was.
    redactions: list[Redaction] = field(default_factory=list)


def _redact_credential_url(value: str) -> str | None:
    """Return a cleaned URL when ``value`` is one carrying a credential, else None.

    Deliberately narrow. It fires only on a string that parses as an absolute
    URL *and* whose query actually carries one of the denied parameters, so
    ordinary prose, file paths, and harmless URLs are recorded untouched — the
    corpus exists to be studied, and blanket redaction of every string would
    make it useless. Returning ``None`` for "nothing to do" keeps the caller
    from having to compare before and after.
    """
    if "://" not in value or "?" not in value:
        return None
    parts = urlsplit(value)
    if not parts.scheme or not parts.netloc or not parts.query:
        return None
    names = {name.lower() for name, _ in parse_qsl(parts.query, keep_blank_values=True)}
    if not any(
        name in URL_ONLY_DENIED_QUERY_KEYS or is_suspicious_key(name) for name in names
    ):
        return None
    return redact_url(value)


def _read_method(frame: Any) -> str | None:
    """Read the JSON-RPC method name, tolerating any frame shape."""
    if not isinstance(frame, dict):
        return None
    method = frame.get("method")
    return method if isinstance(method, str) else None


def redact_frame(frame: Any) -> RedactResult:
    """Withhold every credential-bearing value in a decoded JSON-RPC frame.

    Walks the whole frame rather than only the known paths, so a credential
    nested inside a batch or an unexpected envelope shape is still caught.
    """
    redactions: list[Redaction] = []
    method = _read_method(frame)
    denied = _DENY_BY_METHOD.get(method, ()) if method else ()

    def walk(value: Any, path: str) -> Any:
        if isinstance(value, list):
            return [walk(item, f"{path}[{index}]") for index, item in enumerate(value)]
        if value is None or not isinstance(value, dict):
            return value

        out: dict[str, Any] = {}
        for key, child in value.items():
            child_path = f"{path}.{key}" if path else key

            # An explicit rule for this method, matched at the `params` level.
            if key in denied and (path == "params" or path.startswith("params[")):
                redactions.append(Redaction(path=child_path, reason=f"deny-set:{method}"))
                out[key] = REDACTED
                continue

            # The key-name net, applied at any depth.
            if is_suspicious_key(key):
                redactions.append(Redaction(path=child_path, reason="suspicious-key"))
                out[key] = REDACTED
                continue

            # A credential-bearing URL under an innocent key name. The net
            # above reads key names only, so a frame carrying
            # ``{"url": "ws://host/api/ws?token=..."}`` wrote the token to disk
            # verbatim — and KTD11 puts the attach credential in exactly that
            # position, which makes this the shape most likely to appear.
            if isinstance(child, str):
                cleaned = _redact_credential_url(child)
                if cleaned is not None:
                    redactions.append(Redaction(path=child_path, reason="url-credential"))
                    out[key] = cleaned
                    continue

            out[key] = walk(child, child_path)
        return out

    return RedactResult(frame=walk(frame, ""), redactions=redactions)
