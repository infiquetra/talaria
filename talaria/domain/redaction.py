"""What a credential looks like, and how to withhold one — the shared policy.

Every surface in Talaria that can carry gateway-authored text has the same
obligation: no credential-bearing value reaches a frame log, a transcript
export, a status payload, or a probe diagnostic (v0.1 R27, v0.4 R22). Until
v0.4's U5 that policy lived entirely in :mod:`talaria.recorder.redact`, because
the recorder was the only surface that had it. It is no longer — a seam probe's
error message is gateway text on a status line — and the domain may not import
the recorder (ADR-0002: the domain core imports the standard library and its own
package, and nothing else, enforced by
``tests/domain/test_boundary.py``).

So the *policy* moved here and the *frame format* stayed there.
:mod:`talaria.recorder.redact` keeps the method-keyed deny-set, the frame walk
and the hash-chain-facing result types, and imports the names below rather than
restating them. One list of credential-shaped names, one URL rule: a pattern
added here protects every surface on the same commit, which is the property a
second copy would destroy.

Nothing in this module reads a clock, opens a file, or touches a socket. It is
string policy and nothing else.
"""

from __future__ import annotations

import re
from typing import Any
from urllib.parse import SplitResult, parse_qsl, quote, urlencode, urlsplit, urlunsplit

from talaria.domain.normalize import clip_detail_line, coerce_text

REDACTED = "[redacted]"

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
#:
#: Membership is tested against a **lowercased** key. Every other name-matching
#: rule in this module is case-insensitive (the patterns carry ``re.IGNORECASE``,
#: ``_squashed_key`` lowercases), and a query key is not normalized by anything
#: on the way in, so a set compared against the raw key was the one rule in the
#: file an attacker could evade by pressing shift.
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


def _redact_userinfo(parts: SplitResult) -> tuple[str, bool]:
    """The netloc with any userinfo withheld, and whether anything was withheld.

    The *whole* userinfo component goes, not just the password half. In
    ``https://<token>@host/`` the credential sits in the username position, which
    is an ordinary way to pass a bearer token, so withholding only what follows
    the colon would leak exactly the case worth catching.

    The netloc is rebuilt only when userinfo is actually present. Round-tripping
    every URL through ``hostname``/``port`` would lowercase hosts and normalize
    away the default-port form, changing bytes on URLs carrying no credential at
    all -- and KTD6 compares those bytes against the TypeScript reference.
    """
    if parts.username is None and parts.password is None:
        return parts.netloc, False
    host = parts.hostname or ""
    if ":" in host:
        # An IPv6 literal: urlsplit strips the brackets, urlunsplit needs them
        # back or the address runs into the port separator.
        host = f"[{host}]"
    try:
        port = parts.port
    except ValueError:
        # A non-numeric port. The host half is still worth keeping; guessing at
        # the malformed remainder is not.
        port = None
    netloc = f"{host}:{port}" if port is not None else host
    # Percent-encoded, not the bare marker. `[redacted]@host` puts a literal
    # `[` at the start of the netloc, and `urlsplit` then requires the bracketed
    # span to be a valid IPv6 literal -- so the redacted URL raised ValueError
    # on every subsequent parse, including the frame-log header's own. A corpus
    # is append-only; writing a URL into it that the standard library cannot
    # read back is not a redaction, it is corruption. `%5Bredacted%5D` is also
    # exactly how the marker already appears in a redacted query value, so the
    # on-disk convention stays uniform.
    return f"{quote(REDACTED, safe='')}@{netloc}", True


def redact_url(url: str) -> str:
    """Strip credentials from a URL's userinfo, query string and fragment so it
    is safe to record.

    The fragment is withheld whenever there is one, without inspecting it. It
    carries no key/value structure to filter — ``#token=v`` and ``#v`` are both
    ordinary — so the choice is between withholding every fragment and
    withholding none, and the same reasoning that takes the whole userinfo
    applies: guessing which part of an opaque component is the secret leaks
    exactly the case worth catching. The cost is a document anchor lost from a
    recorded URL, which is a marked hole rather than missing data.
    """
    parts = urlsplit(url)
    if not parts.scheme or not parts.netloc:
        # Not a parseable absolute URL. Withhold it rather than record an
        # unknown string that may itself be a credential.
        return REDACTED

    netloc, changed = _redact_userinfo(parts)

    pairs = parse_qsl(parts.query, keep_blank_values=True)
    query_changed = False
    redacted_pairs: list[tuple[str, str]] = []
    for key, value in pairs:
        # `key.lower()`, not `key`: the members of URL_ONLY_DENIED_QUERY_KEYS are
        # lowercase, query keys are not case-normalized by anything, and
        # `is_suspicious_key` — the other half of this condition — has matched
        # case-insensitively all along. A raw comparison here withheld `?ticket=`
        # and recorded `?Ticket=` verbatim.
        if is_suspicious_key(key) or key.lower() in URL_ONLY_DENIED_QUERY_KEYS:
            redacted_pairs.append((key, REDACTED))
            query_changed = True
        else:
            redacted_pairs.append((key, value))
    # Percent-encoded, matching how the marker already appears in a redacted
    # userinfo and query value, so the on-disk convention stays uniform and a
    # `jq` search for `%5Bredacted%5D` finds every component it was applied to.
    fragment = quote(REDACTED, safe="") if parts.fragment else ""
    changed = changed or query_changed or bool(parts.fragment)

    if not changed:
        return url

    # No `safe` override: JS's `URLSearchParams.toString()` percent-encodes
    # `[` and `]` too (`docs/formats/frame-log.md`'s own header example is
    # `token=%5Bredacted%5D`), so the default `quote_via=quote_plus` matches
    # the reference byte-for-byte on the parts that matter for KTD6 (an
    # exact-value comparison is only made for *un*-redacted query values;
    # redacted ones are compared by presence/state only).
    # Re-encode the query only when a value in it was actually withheld.
    # `urlencode` normalizes percent-escapes, so re-encoding an untouched query
    # would change bytes KTD6 compares for equality.
    new_query = urlencode(redacted_pairs) if query_changed else parts.query
    return urlunsplit((parts.scheme, netloc, parts.path, new_query, fragment))


# ── Free prose, as opposed to a JSON key/value tree (v0.4 U5, R22) ───────


_URLISH = re.compile(r"\S*://\S*")
_BEARER = re.compile(r"\bbearer\s+\S+", re.IGNORECASE)
_ASSIGNMENT = re.compile(r"([A-Za-z][A-Za-z0-9_.\- ]{0,39})\s*[:=]\s*(\S+)")


def _sensitive_free_text_key(key: str) -> bool:
    """Whether a ``key=value`` name found in *prose* must have its value withheld.

    Delegates to :func:`is_suspicious_key` rather than restating the patterns.
    The one addition is the space-to-underscore candidate: those patterns anchor
    on ``[-_]`` separators because they were written to match JSON keys, and an
    English error message says ``session token:`` where a key would say
    ``session_token``.
    """
    stripped = key.strip()
    return any(
        is_suspicious_key(candidate) for candidate in (stripped, stripped.replace(" ", "_"))
    )


def redact_probe_detail(value: Any) -> str:
    """Bound and de-credential one gateway-supplied diagnostic sentence (R22).

    A seam probe's error message is gateway-authored text, and R22 puts a probe
    diagnostic on the same footing as a frame log. Three withholding rules,
    applied in order, then one clip:

    1. ``Bearer <blob>`` — the whole blob goes. Nothing following ``Bearer`` is
       safe to keep.
    2. Anything containing ``://`` — handed to :func:`redact_url`, which strips
       userinfo, suspicious query values and every fragment, and withholds an
       unparseable URL-ish token whole. That direction is right here: an
       endpoint is exactly the string an operator pastes a token into.
    3. ``name: value`` / ``name=value`` where the name is credential-shaped.

    Whitespace is collapsed first, so a multi-line message cannot own several
    rows of a one-line surface and so the rules above see single-spaced text.

    This does **not** defang. Control sequences and markup literals stay in the
    string and are neutralized where every other untrusted string in this
    repository is neutralized — :func:`~talaria.ui.literal.literal_text`, at the
    render boundary (R23). Doing it here would put a second, divergent escaping
    rule below the seam.
    """
    text = " ".join(coerce_text(value).split())
    if not text:
        return ""
    text = _BEARER.sub(REDACTED, text)
    text = _URLISH.sub(lambda match: redact_url(match.group(0)), text)
    text = _ASSIGNMENT.sub(
        lambda match: (
            f"{match.group(1)}={REDACTED}"
            if _sensitive_free_text_key(match.group(1))
            else match.group(0)
        ),
        text,
    )
    return clip_detail_line(text)
