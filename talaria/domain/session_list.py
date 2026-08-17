"""Decode a ``session.list`` reply into the ``/sessions`` picker's rows (R7, R10).

The gateway's handler (``tui_gateway/methods_session.py:162-208`` at
``BASELINE_PIN``) answers with a single top-level key, pinned in
``talaria/domain/compat.py``::

    {"sessions": [{"id": ..., "title": ..., "preview": ..., "started_at": ...,
                    "message_count": ..., "source": ...}, ...]}

That reply's own row shape is **not** in ``MethodBaseline`` — the same design
note U6's history decoder documents applies here: ``MethodBaseline`` stays
top-level by design (``talaria/domain/compat.py:379``), so a shape nested one
level down belongs in a decoder contract instead. This module, and its
contract test in ``tests/domain/test_session_list.py``, are that contract for
``session.list``.

``"id"`` is the **stored** session id (``tui_gateway/methods_session.py:197``)
— the durable identity a later ``session.resume`` asks for, and the one the
picker's rows are identified by and matched against
:attr:`~talaria.domain.state.SessionState.session_key` for the focused-session
marker (R7). It is read here under a domain field named ``session_id`` for
symmetry with the rest of the codebase's vocabulary, not because the wire key
changed.

No live ``session.list`` recording exists in the local corpus at the time this
module was written — the fixture in its contract test is transcribed from the
handler at the pin above rather than recorded from a live reply, and is
labelled ``source-transcribed`` there for that reason (mirroring
``tests/domain/test_history_decoder.py``'s provenance discipline). A live
capture that carries this reply should replace it and move the label to
``recorded``.
"""

from __future__ import annotations

import math
from collections.abc import Mapping
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any


@dataclass(frozen=True)
class SessionSummary:
    """One session as ``session.list`` reports it, decoded and never guessed at.

    Every field below degrades to an empty or zero value rather than raising
    when the wire disagrees with the pin — the same AE9-adjacent posture
    :mod:`~talaria.domain.models_catalog` takes for a malformed admin reply.
    The one field that is *not* defaulted is :attr:`session_id`: a row with no
    id names nothing a later ``session.resume`` could ask for, so
    :func:`decode_session_list` drops such a row rather than manufacturing a
    picker entry with an empty target.
    """

    session_id: str
    title: str = ""
    preview: str = ""
    started_at: float = 0.0
    message_count: int = 0
    source: str = ""


@dataclass(frozen=True)
class SessionDirectory:
    """Every session the gateway listed, in the order it sent them."""

    sessions: tuple[SessionSummary, ...] = ()

    @property
    def is_empty(self) -> bool:
        return not self.sessions


def _text(value: Any) -> str:
    return value if isinstance(value, str) else ""


def _number(value: Any) -> float:
    """A ``datetime``-representable ``started_at``, or 0.0 (this module's own
    "missing" value).

    A NaN, an infinity, or a magnitude ``datetime`` cannot represent (its year
    is bounded to 1-9999) raises inside ``datetime.fromtimestamp``, and a
    listing row carrying one used to crash the ``/sessions`` picker's own
    construction before the operator ever saw a dialog (P2, U7 round two). The
    picker no longer formats this stamp — v0.4's registry-backed rows render
    frame-clock ages instead (KTD12) — but the guard stays here, where the
    hazard actually is: this decoder's output is what any renderer, present or
    future, is entitled to hand to ``datetime``. It is verified against the
    exact conversion rather than a hand-computed bound, because the safe range
    is a property of ``datetime`` and not one this module should keep in sync
    by hand. Folded into the same 0.0 already returned for a missing or
    wrong-typed value.
    """
    if isinstance(value, bool):
        return 0.0
    if not isinstance(value, (int, float)):
        return 0.0
    number = float(value)
    if math.isnan(number) or math.isinf(number):
        return 0.0
    try:
        datetime.fromtimestamp(number, tz=UTC)
    except (OverflowError, ValueError, OSError):
        return 0.0
    return number


def _count(value: Any) -> int:
    if isinstance(value, bool):
        return 0
    if isinstance(value, int):
        return value
    return 0


@dataclass(frozen=True)
class ActiveSessionSummary:
    """One session as ``session.active_list`` reports it (U3, KTD2).

    The wire row is exactly ``{current, id, last_active, message_count, model,
    preview, session_key, started_at, status, title}`` — verified live on
    2026-08-17 (U1's topology verification), with epoch-seconds floats for the
    timestamps and ``started_at`` being session creation, not prompt start.
    ``session_key`` is the durable identity (the registry key half);
    ``session_id`` (wire ``id``) is the runtime id, a routing alias.

    ``status`` is kept **verbatim** (KTD10: gateway words, re-encoded never
    translated) — including a word outside the known four, which a renderer
    defangs and shows literally rather than this decoder guessing at.
    ``last_active`` and ``started_at`` are gateway wall-clock values; ages are
    never derived from them (KTD12 — ages ride the frame clock), they exist
    for display formatting only.
    """

    session_id: str
    session_key: str = ""
    title: str = ""
    preview: str = ""
    status: str = ""
    model: str = ""
    current: bool = False
    started_at: float = 0.0
    last_active: float = 0.0
    message_count: int = 0

    @property
    def durable_id(self) -> str:
        """The registry-key half: ``session_key`` when known, else the id."""
        return self.session_key or self.session_id


@dataclass(frozen=True)
class ActiveSessionDirectory:
    """Every live session the gateway reported, in the order it sent them."""

    sessions: tuple[ActiveSessionSummary, ...] = ()

    @property
    def is_empty(self) -> bool:
        return not self.sessions


def decode_active_list(result: Any) -> ActiveSessionDirectory:
    """Turn a ``session.active_list`` reply into a directory, degrading
    rather than raising — the same posture as :func:`decode_session_list`,
    for the same reason: this runs on a poll loop, and a malformed reply
    must mark data stale (the caller's job), never crash the poller. A row
    with no usable ``id`` names nothing routable and is dropped."""
    if not isinstance(result, Mapping):
        return ActiveSessionDirectory()
    raw = result.get("sessions")
    if not isinstance(raw, list):
        return ActiveSessionDirectory()
    rows: list[ActiveSessionSummary] = []
    for row in raw:
        if not isinstance(row, Mapping):
            continue
        session_id = _text(row.get("id"))
        if not session_id:
            continue
        rows.append(
            ActiveSessionSummary(
                session_id=session_id,
                session_key=_text(row.get("session_key")),
                title=_text(row.get("title")),
                preview=_text(row.get("preview")),
                status=_text(row.get("status")),
                model=_text(row.get("model")),
                current=row.get("current") is True,
                started_at=_number(row.get("started_at")),
                last_active=_number(row.get("last_active")),
                message_count=_count(row.get("message_count")),
            )
        )
    return ActiveSessionDirectory(sessions=tuple(rows))


def decode_session_list(result: Any) -> SessionDirectory:
    """Turn a ``session.list`` reply into a listing, degrading rather than raising.

    A reply that is not a mapping, or whose ``sessions`` key is not a list,
    yields an empty directory rather than an exception — this runs on a
    picker's open path and a malformed reply must not be a crash. A row that
    is not itself a mapping, or that carries no ``"id"``, is dropped silently
    rather than kept as an unselectable placeholder: unlike a resumed
    transcript line (U6), a listing row nobody can select is not information
    the operator needs to see, it is noise in a picker.
    """
    if not isinstance(result, Mapping):
        return SessionDirectory()
    raw = result.get("sessions")
    if not isinstance(raw, list):
        return SessionDirectory()
    rows: list[SessionSummary] = []
    for row in raw:
        if not isinstance(row, Mapping):
            continue
        session_id = _text(row.get("id"))
        if not session_id:
            continue
        rows.append(
            SessionSummary(
                session_id=session_id,
                title=_text(row.get("title")),
                preview=_text(row.get("preview")),
                started_at=_number(row.get("started_at")),
                message_count=_count(row.get("message_count")),
                source=_text(row.get("source")),
            )
        )
    return SessionDirectory(sessions=tuple(rows))


__all__ = [
    "ActiveSessionDirectory",
    "ActiveSessionSummary",
    "SessionDirectory",
    "SessionSummary",
    "decode_active_list",
    "decode_session_list",
]
