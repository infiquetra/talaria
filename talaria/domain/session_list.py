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

    ``started_at`` reaches :func:`~talaria.ui.picker.format_session_detail`,
    which hands it to ``datetime.fromtimestamp`` — NaN, an infinity, or a
    magnitude ``datetime`` cannot represent (its year is bounded to 1-9999)
    raises there and would crash the picker's own construction before the
    operator ever sees a dialog (P2, U7 round two). Verified against the
    exact conversion the render side performs, rather than a hand-computed
    bound, because the safe range is a property of ``datetime`` and not one
    this module should have to keep in sync with it by hand. Folded into the
    same 0.0 already returned for a missing or wrong-typed value — an
    unrenderable timestamp is not a new kind of failure the picker has to
    learn, it is this one.
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


__all__ = ["SessionDirectory", "SessionSummary", "decode_session_list"]
