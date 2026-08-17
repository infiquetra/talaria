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

Further divergences have been added since, each after something put credentials
on disk through this boundary -- an adversarial probe on 2026-08-03, a
code-review gate on 2026-08-05. Each is enumerated here for the same reason as
the first: an unlisted divergence is what KTD6 forbids. (This paragraph read
"two further divergences ... both are enumerated" until 2026-08-05. It was
written when the list below held two entries and was not updated as the list
grew to five -- the same drift the enumeration exists to prevent, in the prose
that announces it.)

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

3. *URL userinfo is withheld, not only the query string.* ``urlsplit`` hands
   ``user:password@host`` back inside ``netloc`` and :func:`urlunsplit` writes it
   out verbatim, so a URL cleaned only through its query carried its credentials
   to disk intact -- including in the frame-log *header*, the one place
   :func:`redact_url` was already wired up. See :func:`_redact_userinfo`.

4. *A URL is examined for credentials even when it has no query string.* The
   check added in (2) returned early unless the value contained a ``?``, so an
   operator-configured endpoint of the form ``http://user:pass@host/`` was
   recorded whole. At the pinned revision ``browser.manage`` returns exactly
   this value on an ordinary status call (``tui_gateway/server.py:13405``
   resolves ``BROWSER_CDP_URL`` or ``browser.cdp_url`` and hands it back through
   ``methods_tools.py:1349``), so a remote CDP endpoint with basic-auth
   credentials reaches a frame body with no query anywhere in it.

5. *A URL fragment is withheld whole, whenever there is one.* ``urlsplit`` hands
   everything after ``#`` back as one opaque string and :func:`urlunsplit` wrote
   it out verbatim, so an endpoint of the form ``ws://host/api/ws#token=...``
   reached the frame-log header with the value intact -- the third and last
   position a URL can carry a credential in, and the only one nothing covered.
   There is no key to match here as there is in a query, so the choice is
   all-or-nothing and the security property wins again: any fragment at all is
   replaced by the marker.

   This is the widest of the five, and knowingly so. Divergences 1 through 4
   fire only on a credential-shaped name or position; this one fires on a
   fragment that plainly holds no credential, costing a document anchor on an
   unrelated ``https`` URL quoted inside a frame body. It is paid as a recorded
   ``url-credential`` redaction rather than a silent edit, so the corpus shows
   the hole. Talaria's own ``ws``/``wss`` endpoints lose nothing at all, because
   a fragment is a client-side selector that is never sent on the wire.

These make the Python redactor a wider superset; none withholds anything the
TypeScript reference withholds differently, and the usage counters this module
exists to preserve (``max_tokens`` and its siblings) are unaffected -- pinned by
the over-redaction controls in ``tests/recorder/test_redact.py``.

Divergences 2 and 4 were, for a time, enumerated *here* and nowhere the harness
could see. The equivalence corpus contained no frame carrying a URL at all, and
``compare_records`` compared frame bodies with a flat equality that had no
authorized-divergence path -- so the first frame-body URL redaction would have
been reported as a port bug, and the claim above that the relation is "pinned by
a test rather than drifting" was false for exactly these two entries. The fixture
now carries both shapes and the comparator authorizes them by reason, with its
own independent expectation of what a redacted URL looks like.

**Known and deliberately not covered.** A bearer capability carried in a URL's
*path* rather than its userinfo or query -- the concrete
``ws://<host>:9222/devtools/browser/<GUID>`` form Chrome hands out, where the GUID
alone drives the browser -- is still recorded verbatim.

Do not read that as loopback-scoped. Loopback is the *default* CDP host, not a
constraint: at the pin, ``BROWSER_CDP_URL`` is documented to operators as
accepting "any running Chromium-family browser", so a remote CDP endpoint is an
ordinary configuration today. The exposure is real; it is the available fixes
that are worse than the defect. A Hermes-shaped path rule protects one known
shape while implying paths are handled, and a "high-entropy path segment"
heuristic would redact the commit SHAs and resource ids the corpus exists to
study -- the same over-redaction failure the key-name net is anchored to avoid.

The leading candidate -- withholding the path of non-loopback ``ws``/``wss``
URLs -- is blocked on the KTD6 comparator, which can express an authorized
divergence in userinfo, query-key names and (since divergence 5) the fragment,
but not in paths. That is now a smaller obstacle than it reads: divergence 5
widened the comparator by exactly the move a path rule would need, so the
blocker is the over-redaction question above, not the harness. Tracked with both
revisit triggers in ``docs/engineering-journal/QUEUED.md``.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any
from urllib.parse import parse_qsl, urlsplit

# The credential-name deny-set, the URL rule and the withholding marker moved to
# ``talaria/domain/redaction.py`` in v0.4's U5 and are imported rather than
# restated. The reason is a boundary, not tidiness: a seam probe's diagnostic is
# gateway text that has to obey the same policy, and the domain may not import
# this package (ADR-0002, enforced by ``tests/domain/test_boundary.py``). What
# stayed here is what is specific to the *frame format* — the method-keyed
# deny-set below, the frame walk, and the hash-chain-facing result types. These
# names are re-exported by this import, so every existing
# ``from talaria.recorder.redact import ...`` call site is unchanged, including
# the TypeScript-equivalence harness.
from talaria.domain.redaction import (
    REDACTED,
    SENSITIVE_KEY_PATTERNS,
    URL_ONLY_DENIED_QUERY_KEYS,
    is_suspicious_key,
    redact_url,
)

__all__ = [
    "REDACTED",
    "SENSITIVE_KEY_PATTERNS",
    "URL_ONLY_DENIED_QUERY_KEYS",
    "RedactResult",
    "Redaction",
    "is_suspicious_key",
    "redact_frame",
    "redact_url",
]

#: A value withheld from the recording. The original never reaches disk.

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
    URL *and* carries a credential in one of the two positions a URL puts them:
    userinfo, or a query parameter with a denied name. Ordinary prose, file
    paths, and harmless URLs are recorded untouched — the corpus exists to be
    studied, and blanket redaction of every string would make it useless.
    Returning ``None`` for "nothing to do" keeps the caller from having to
    compare before and after.

    The userinfo check comes first and deliberately does not require a query
    string. Requiring one is what let ``http://user:pass@cdp.example/`` through:
    it is a complete credential with no ``?`` anywhere in it, and it is the shape
    an operator's configured CDP override actually takes (see divergence 4 in the
    module docstring).

    A fragment is the third such position, and this gate has to know about it.
    :func:`redact_url` withholds fragments, but a value that never reaches
    :func:`redact_url` is never withheld — so widening the redactor alone would
    have closed the frame-log *header* and left every URL in a frame *body*
    untouched, while a unit test of :func:`redact_url` reported the fix working
    (see divergence 5 in the module docstring).
    """
    if "://" not in value:
        return None
    try:
        parts = urlsplit(value)
    except ValueError:
        # Not parseable as a URL at all (a bad IPv6 literal, say). It is then
        # not a URL-shaped credential either, and the key-name net still applies.
        return None
    if not parts.scheme or not parts.netloc:
        return None
    if parts.username is None and parts.password is None and not parts.fragment:
        if not parts.query:
            return None
        names = {name.lower() for name, _ in parse_qsl(parts.query, keep_blank_values=True)}
        if not any(
            name in URL_ONLY_DENIED_QUERY_KEYS or is_suspicious_key(name) for name in names
        ):
            return None

    cleaned = redact_url(value)
    # The single exit, and the reason it is single: this function decides
    # whether to fire on a lowercased view of the query keys, and `redact_url`
    # decides what to withhold on its own. When those two disagreed, the caller
    # got the value back untouched *and* appended a Redaction saying the
    # credential had been withheld — a corpus that documents a redaction it did
    # not perform, which is worse than the leak, because the leak is at least
    # visible to anyone who reads the frame. Reporting is now derived from the
    # bytes rather than from the decision to look at them.
    return cleaned if cleaned != value else None


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

    **The deny-set travels with the walk.** It used to be resolved once, from the
    outermost object, and applied only where the dotted path was exactly
    ``params`` or ``params[...]``. That coupled a security rule to a string
    comparison on position, and every shape that moved the frame off the top
    level silently disarmed it: a batch, a wrapping envelope, and — with no
    batching involved at all — an ordinary ``params.inner.answer``. ``answer``,
    ``value`` and ``text`` are deny-set-only by design, so nothing else caught
    them. The method is now re-read from each object that carries one, and it
    governs that object's own ``params`` subtree to any depth.
    """
    redactions: list[Redaction] = []

    def walk(
        value: Any,
        path: str,
        denied: tuple[str, ...],
        method: str | None,
        in_params: bool,
    ) -> Any:
        if isinstance(value, list):
            return [
                walk(item, f"{path}[{index}]", denied, method, in_params)
                for index, item in enumerate(value)
            ]
        if value is None or not isinstance(value, dict):
            return value

        # An object carrying its own `method` governs its own subtree. Only a
        # method that is actually in the deny-set takes over, so an inner
        # `{"method": "GET"}` -- a plain HTTP verb under some unrelated key --
        # cannot clear a deny context established above it.
        local_method = _read_method(value)
        if local_method is not None and local_method in _DENY_BY_METHOD:
            denied = _DENY_BY_METHOD[local_method]
            method = local_method
            in_params = False

        out: dict[str, Any] = {}
        for key, child in value.items():
            child_path = f"{path}.{key}" if path else key

            # An explicit rule for this method, anywhere beneath its `params`.
            if in_params and key in denied:
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
                # `cleaned != child` is belt to `_redact_credential_url`'s own
                # braces. The invariant is that `redactions` describes bytes that
                # actually changed, and it is cheap enough to enforce at the one
                # place the record is written rather than trusting every rule
                # that feeds it to have got its own answer right.
                if cleaned is not None and cleaned != child:
                    redactions.append(Redaction(path=child_path, reason="url-credential"))
                    out[key] = cleaned
                    continue

            # Once inside a governed `params`, stay inside it for the whole
            # subtree — that is what makes `params.inner.answer` reachable.
            out[key] = walk(child, child_path, denied, method, in_params or key == "params")
        return out

    return RedactResult(frame=walk(frame, "", (), None, False), redactions=redactions)
